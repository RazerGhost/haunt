//! The scans: the shown repository's scanner (started, asked, restarted when trust or its tooling changes), its
//! watchers, and the kept scans and scanners of the other tabs.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use serde_json::json;

use super::{git_dir, Inner, Kept, Reply, Repos, Shown, KEPT, RUNNING};
use crate::paths;
use crate::sidecar::{Said, Shape, Sidecar};
use crate::watch::{self, Change, GitPart};

impl Repos {
    /// The shown repository's scanner, as the generation it was started as.
    #[cfg(test)]
    pub fn scanner(&self) -> Option<u64> {
        let inner = self.inner.lock();
        inner.shown.as_ref().filter(|shown| shown.sidecar.is_some()).map(|shown| shown.generation)
    }

    /// Asks the reader for the functions of a TypeScript-family file's text (another version of `path` in `repo`). It
    /// scans nothing, so it answers at once even while the repository's sidecar works out a call graph; it is started
    /// when first needed, and again if it stopped.
    pub fn read_functions(&self, repo: &str, path: &str, text: &str) -> Result<Receiver<Reply>, String> {
        self.require(repo)?;
        let mut reader = self.reader.lock();
        if !reader.as_ref().is_some_and(Sidecar::running) {
            let runtime = self.context.runtime.clone()?;
            let script = self.context.script.with_file_name("read.ts");
            let started = Sidecar::start(
                &runtime,
                &script,
                "",
                false,
                self.context.log.as_deref(),
                None,
                |_| {},
                Arc::new(|_: &str| Err("The reader asks nothing.".to_string())),
            )
            .map_err(|error| {
                said!(
                    "repos.readerNotStarted",
                    "{runtime} could not start the reader: {error}",
                    runtime = runtime.name(),
                    error = error
                )
            })?;
            *reader = Some(started);
        }
        let reader = reader.as_ref().expect("started");
        Ok(reader.ask(json!({ "type": "functions", "root": repo, "path": path, "text": text })))
    }

    pub fn rescan(&self, repo: &str) -> Result<(), String> {
        let mut inner = self.inner.lock();
        self.check(&inner, repo)?;
        self.scan(&mut inner, "a rescan");
        Ok(())
    }

    /// The latest structure as JSON text, or None until the first scan is in. It never waits: a call held open would
    /// take one of the few connections the window has, and the `scan` event says when to ask again.
    pub fn structure(&self, repo: &str) -> Result<Option<String>, String> {
        let inner = self.inner.lock();
        self.check(&inner, repo)?;
        Ok(inner.shown.as_ref().expect("checked").code.structure.clone())
    }

    /// The call graph of the latest structure (`scannedAt`, `calls`) as JSON text, or null until it is worked out.
    pub fn calls(&self, repo: &str) -> Result<String, String> {
        let inner = self.inner.lock();
        self.check(&inner, repo)?;
        Ok(inner.shown.as_ref().and_then(|shown| shown.code.calls.clone()).unwrap_or_else(|| "null".into()))
    }

    /// Asks the shown repository's scanner something (`sidecar/serve.ts`), and waits for its answer: it answers after
    /// the first structure, between the parts of a scan.
    pub fn ask_scanner(&self, repo: &str, request: serde_json::Value) -> Result<String, String> {
        let asked = {
            let inner = self.inner.lock();
            self.check(&inner, repo)?;
            let shown = inner.shown.as_ref().unwrap();
            if shown.code.structure.is_none() {
                return Err(said!("repos.stillScanning", "The code is still being scanned: try again in a moment."));
            }
            match &shown.sidecar {
                Some(sidecar) => sidecar.ask(request),
                // Scanned here, without a runtime: answered here too.
                None if self.context.runtime.is_err() => {
                    return crate::code::scan::answer(&request, shown.code.structure.as_deref().unwrap_or("null"));
                }
                None => return Err(said!("repos.scannerNotRunning", "The scanner is not running.")),
            }
        };
        asked
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|_| said!("repos.scannerLate", "The scanner did not answer in time."))?
    }

    /// A file of the shown repository's last scan, as a path inside it; the editor and the source view take only these.
    pub fn scanned(&self, repo: &str, path: &str) -> Result<PathBuf, String> {
        let inner = self.inner.lock();
        self.check(&inner, repo)?;
        let known = inner
            .shown
            .as_ref()
            .and_then(|shown| shown.code.shape.as_ref())
            .is_some_and(|shape| shape.files.iter().any(|file| file.path == path));
        if !known {
            return Err(said!("repos.fileNotScanned", "That file is not in the scan."));
        }
        paths::inside(repo, path).ok_or_else(|| said!("repos.fileOutside", "That file is not in the repository."))
    }

    /// Shows `root`: the shown repository's scan is kept, `root`'s kept scan (if any) shows at once, and a scanner
    /// and watchers start for it.
    pub(super) fn show(&self, inner: &mut Inner, root: &str) {
        // Its config may have changed while it was not shown.
        crate::git::forget_config(root);
        if let Some(old) = inner.shown.take() {
            if old.code.structure.is_some() || old.sidecar.is_some() {
                inner.kept.retain(|kept| kept.root != old.root);
                let scanner = old.sidecar.map(|sidecar| (sidecar, old.generation));
                inner.kept.push(Kept { root: old.root, code: old.code, scanner });
                let excess = inner.kept.len().saturating_sub(KEPT);
                inner.kept.drain(..excess);
                // Only the last few keep their scanner: dropping one stops its process.
                let stopping = inner.kept.len().saturating_sub(RUNNING);
                for kept in &mut inner.kept[..stopping] {
                    kept.scanner = None;
                }
            }
        }
        let kept = inner.kept.iter_mut().find(|kept| kept.root == root);
        let code = kept.as_ref().map(|kept| kept.code.clone()).unwrap_or_default();
        let scanner = kept.and_then(|kept| kept.scanner.take()).filter(|(sidecar, _)| sidecar.running());
        let base = PathBuf::from(root);
        let on = self.handler(root);
        let git_dir = git_dir(root);
        let (sidecar, generation) = match scanner {
            Some((sidecar, generation)) => (Some(sidecar), generation),
            None => (None, 0),
        };
        let reuse = sidecar.is_some();
        inner.shown = Some(Shown {
            root: root.to_owned(),
            generation,
            sidecar,
            code,
            watch_root: watch::root(&base, on.clone()),
            watch_folders: Vec::new(),
            watching: Vec::new(),
            watch_git: git_dir.map(|dir| watch::git(&dir, on.clone())).unwrap_or_default(),
            #[cfg(debug_assertions)]
            watch_sidecar: self.context.script.parent().and_then(|dir| watch::sidecar(dir, on.clone())),
        });
        self.events.send(Some(root), json!({ "type": "root", "root": root }));
        if reuse {
            // Its scanner kept running: a scan reads only the files that changed since.
            self.scan(inner, "start");
        } else {
            self.restart(inner, "start");
        }
        if let Some(shape) = inner.shown.as_ref().and_then(|shown| shown.code.shape.clone()) {
            self.watch_folders(inner, &shape, false);
        }
    }

    /// What a file change in the shown repository asks for.
    fn handler(&self, root: &str) -> impl Fn(Change) + Send + Sync + Clone + 'static {
        let me = self.me.clone();
        let root = root.to_owned();
        move |change: Change| {
            let Some(repos) = me.upgrade() else { return };
            let mut inner = repos.inner.lock();
            if inner.shown.as_ref().map(|shown| shown.root.as_str()) != Some(root.as_str()) {
                return;
            }
            match change {
                Change::Scan(reason) => repos.scan(&mut inner, &reason),
                Change::NewFolder(folder) => {
                    if let Some(shape) = inner.shown.as_ref().and_then(|shown| shown.code.shape.clone()) {
                        repos.watch_folders(&mut inner, &shape, true);
                    }
                    repos.scan(&mut inner, &folder);
                }
                Change::Restart(reason) => repos.restart(&mut inner, &reason),
                Change::GitConfig => {
                    crate::git::forget_config(&root);
                    let parts: Vec<&str> = GitPart::ALL.iter().map(|part| part.name()).collect();
                    repos.events.send(Some(&root), json!({ "type": "git", "parts": parts }));
                }
                Change::Git(parts) => {
                    let parts: Vec<&str> = parts.iter().map(|part| part.name()).collect();
                    if !repos.just_announced(&root, &parts) {
                        repos.events.send(Some(&root), json!({ "type": "git", "parts": parts }));
                    }
                }
            }
        }
    }

    fn scan(&self, inner: &mut Inner, reason: &str) {
        let Some(shown) = inner.shown.as_ref() else { return };
        match &shown.sidecar {
            Some(sidecar) => {
                sidecar.tell(&json!({ "type": "scan" }));
                self.events.send(Some(&shown.root), json!({ "type": "scanning", "reason": reason }));
            }
            None => self.restart(inner, reason),
        }
    }

    /// A new scanner process for the shown repository (the old one, if any, is stopped), and a scan.
    pub(super) fn restart(&self, inner: &mut Inner, reason: &str) {
        inner.generation += 1;
        let generation = inner.generation;
        let Some(shown) = inner.shown.as_mut() else { return };
        shown.generation = generation;
        shown.sidecar = None;
        let root = shown.root.clone();
        let runtime = match &self.context.runtime {
            Ok(runtime) => runtime.clone(),
            Err(problem) => {
                let trusted = inner.settings.trusts(&root);
                self.scan_here(&root, generation, trusted, problem.clone(), reason);
                return;
            }
        };
        let me = self.me.clone();
        let from = root.clone();
        let started = Sidecar::start(
            &runtime,
            &self.context.script,
            &root,
            inner.settings.trusts(&root),
            self.context.log.as_deref(),
            self.context.cache.as_deref(),
            move |said| {
                if let Some(repos) = me.upgrade() {
                    repos.heard(&from, generation, said);
                }
            },
            {
                let root = root.clone();
                Arc::new(move |line: &str| crate::code::others::answer(&root, line))
            },
        );
        let shown = inner.shown.as_mut().expect("shown");
        match started {
            Ok(sidecar) => {
                sidecar.tell(&json!({ "type": "scan" }));
                shown.sidecar = Some(sidecar);
                self.events.send(Some(&root), json!({ "type": "scanning", "reason": reason }));
            }
            Err(error) => {
                let problem = said!(
                    "repos.scannerNotStarted",
                    "{runtime} could not start the scanner: {error}",
                    runtime = runtime.name(),
                    error = error
                );
                self.events.send(Some(&root), json!({ "type": "error", "message": problem }));
            }
        }
    }

    /// Without a JavaScript runtime, the scan done here (code/scan.rs) for the languages Rust reads, on a thread of
    /// its own; when the repository has none of those, why nothing is scanned.
    fn scan_here(&self, root: &str, generation: u64, trusted: bool, problem: String, reason: &str) {
        self.events.send(Some(root), json!({ "type": "scanning", "reason": reason }));
        let me = self.me.clone();
        let root = root.to_owned();
        std::thread::spawn(move || {
            let scanned = crate::code::scan::scan(&root, trusted);
            let Some(repos) = me.upgrade() else { return };
            match scanned {
                Some(scanned) => {
                    let scanned_at = scanned.shape.scanned_at.clone();
                    repos.heard(&root, generation, Said::Structure(scanned.structure, scanned.shape));
                    repos.heard(&root, generation, Said::Calls(scanned.calls, scanned_at));
                }
                None => repos.events.send(Some(&root), json!({ "type": "error", "message": problem })),
            }
        });
    }

    /// What a scanner said, if it is still the shown repository's current one.
    fn heard(&self, root: &str, generation: u64, said: Said) {
        let mut inner = self.inner.lock();
        let current = inner.shown.as_ref().is_some_and(|shown| shown.root == root && shown.generation == generation);
        if !current {
            return;
        }
        match said {
            Said::Structure(structure, shape) => {
                let shown = inner.shown.as_mut().expect("current");
                // Keep the last call graph until the new one is in, rather than an empty view after every save.
                shown.code.structure = Some(structure.clone());
                let scanned_at = shape.scanned_at.clone();
                shown.code.shape = Some(shape.clone());
                self.watch_folders(&mut inner, &shape, false);
                self.events.send(Some(root), json!({ "type": "scan", "scannedAt": scanned_at, "complete": false }));
            }
            Said::Calls(message, scanned_at) => {
                let shown = inner.shown.as_mut().expect("current");
                if shown.code.shape.as_ref().map(|shape| shape.scanned_at.as_str()) != Some(scanned_at.as_str()) {
                    return;
                }
                shown.code.calls = Some(message);
                self.events.send(Some(root), json!({ "type": "scan", "scannedAt": scanned_at, "complete": true }));
            }
            Said::Error(message) => {
                eprintln!("[haunt] scan failed: {message}");
                let first = message.lines().next().unwrap_or(&message).to_owned();
                self.events.send(Some(root), json!({ "type": "error", "message": first }));
            }
            Said::Exited => {
                let shown = inner.shown.as_mut().expect("current");
                shown.sidecar = None;
                self.events.send(Some(root), json!({ "type": "error", "message": said!("repos.scannerStopped", "The scanner stopped. Save a file or rescan to start it again.") }));
            }
        }
    }

    /// Watches the folders the last scan's files are in, when they changed (or `again`: a folder was made). When the
    /// system refused more watches, the window says so once per repository shown.
    fn watch_folders(&self, inner: &mut Inner, shape: &Shape, again: bool) {
        let Some(shown) = inner.shown.as_mut() else { return };
        let mut folders = shape.watch.clone();
        folders.sort();
        if folders == shown.watching && !again {
            return;
        }
        let on = self.handler(&shown.root);
        shown.watch_folders = Vec::new();
        shown.watch_folders = watch::folders(Path::new(&shown.root), &folders, shape.restart_on.clone(), on);
        shown.watching = folders;
        if watch::limit_reached() {
            let root = shown.root.clone();
            self.events.send(
                Some(&root),
                json!({ "type": "notice", "message": said!("repos.watchLimit", "The system's limit on watched folders was reached, so saving some files will not rescan. On Linux, raise it with: sudo sysctl fs.inotify.max_user_watches=524288") }),
            );
        }
    }
}
