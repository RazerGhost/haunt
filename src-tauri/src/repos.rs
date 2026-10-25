//! The repositories: the open tabs, the one shown with its scanner process and watchers, trust, and the last scan of a
//! few other tabs, kept in memory so going back to one shows it at once while it is scanned again. The last few of those
//! keep their scanner running too: going back asks it to scan again, which reads only what changed, instead of starting
//! a new one that reads everything. Every call names its repository; a call for one that is not shown is refused, so an
//! answer can never land in the wrong tab.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use specta::Type;

use crate::events::Events;
use crate::paths;
use crate::settings::{PullChoice, Settings, FETCH_EVERY};
use crate::sidecar::{Said, Shape, Sidecar};
use crate::startup::Runtime;
use crate::watch::{self, Change, GitPart, Watcher};

/// How many other tabs keep their last scan in memory.
const KEPT: usize = 4;
/// How many of those keep their scanner running (each is a Node, Bun or Deno process holding a TypeScript program).
const RUNNING: usize = 2;

pub type Reply = Result<String, String>;

/// The repository shown, the open tabs, the recent ones, whether its own code may run and what it would run.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RootInfo {
    pub root: Option<String>,
    pub tabs: Vec<String>,
    pub recent: Vec<String>,
    pub trusted: bool,
    /// What the folder would run if trusted; empty when it has nothing to run.
    pub wants: Vec<Wanted>,
    /// The tabs that are linked worktrees of a repository (their `.git` is a file), to mark them.
    pub worktrees: Vec<String>,
    /// The profile in use, and every profile's name.
    pub profile: String,
    pub profiles: Vec<String>,
    /// How the profile in use looks.
    pub appearance: crate::settings::Appearance,
    /// The open tabs that are pinned or coloured, by folder.
    pub tab_looks: HashMap<String, crate::settings::TabLook>,
}

/// A recent repository, as the open screen shows it: its folder's name, and, when it is a git repository, its branch,
/// its uncommitted files and its last commit.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepo {
    pub path: String,
    pub name: String,
    /// The folder is still there, and a project or a git repository.
    pub openable: bool,
    pub branch: Option<String>,
    pub changes: Option<u32>,
    pub last_commit: Option<crate::git::types::CommitInfo>,
}

impl RecentRepo {
    fn read(path: &str) -> RecentRepo {
        let openable = paths::openable(Path::new(path));
        let git = openable && crate::git::diff::is_repository(path);
        RecentRepo {
            path: path.to_owned(),
            name: Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned()),
            openable,
            branch: git.then(|| crate::git::diff::head(path).branch).flatten(),
            changes: git.then(|| crate::git::local_changes(path).ok().flatten()).flatten(),
            last_commit: git
                .then(|| crate::git::first_parents(path, 1).ok().and_then(|commits| commits.into_iter().next()))
                .flatten(),
        }
    }
}

/// What a typed path is, before it is opened: whether it is there, and whether Haunt can open it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderCheck {
    pub exists: bool,
    pub openable: bool,
    pub repository: bool,
}

pub fn check_folder(path: &str) -> FolderCheck {
    match paths::canonical(path) {
        None => FolderCheck { exists: false, openable: false, repository: false },
        Some(root) => FolderCheck {
            exists: true,
            openable: paths::openable(Path::new(&root)),
            repository: Path::new(&root).ancestors().any(|dir| dir.join(".git").exists()),
        },
    }
}

/// Code of a folder that Haunt runs only once the folder is trusted.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Wanted {
    pub kind: WantedKind,
    /// What it is, for the trust banner.
    pub label: String,
    /// Its path, relative to the folder.
    pub path: String,
}

/// What the Settings window shows: the git settings, the trusted folders, and where Haunt keeps its files.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub pull: PullChoice,
    /// Minutes between background fetches; 0 is off.
    pub fetch_every: u32,
    /// Pull request status from gh, glab or tea; off by default.
    pub host_tools: bool,
    /// Looking for a new version of Haunt now and then; off by default.
    pub update_check: bool,
    pub trusted: Vec<TrustedFolder>,
    pub version: String,
    pub folders: AppFolders,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TrustedFolder {
    pub path: String,
    /// The folder is not there now (an unplugged drive, a deleted checkout).
    pub missing: bool,
}

/// Where Haunt keeps its files; null where there is none (the logs while Haunt is worked on).
#[derive(Debug, Clone, Serialize, Type)]
pub struct AppFolders {
    pub settings: String,
    pub cache: Option<String>,
    pub logs: Option<String>,
}

/// One of Haunt's own folders, to open in the file manager.
#[derive(Debug, Clone, Copy, serde::Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AppFolder {
    Settings,
    Cache,
    Logs,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WantedKind {
    Typescript,
    Script,
    /// A git hook: git runs it on a commit, push, checkout… that Haunt makes, in a trusted folder only.
    Hook,
}

/// What a folder would run, to read before trusting it: each file's text (a hook, a script), or what it is.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WantedText {
    pub label: String,
    pub path: String,
    /// Its text, at most `WANTED_TEXT` bytes; empty for what is not a file to read (the TypeScript compiler).
    pub text: String,
    /// What it is when there is no text to show, or what else to know.
    pub note: String,
    pub truncated: bool,
}

/// The most of a hook or script shown before trusting.
const WANTED_TEXT: usize = 64 * 1024;

/// The hooks git runs for what Haunt does: committing, pushing, checking out, merging, rebasing, applying patches.
const HOOKS: [&str; 14] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-merge-commit",
    "post-merge",
    "pre-push",
    "post-checkout",
    "pre-rebase",
    "post-rewrite",
    "reference-transaction",
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
];

/// What Haunt needs to run a scanner.
pub struct Context {
    /// What runs the scanner (Node, Bun or Deno), or why none can (then nothing is scanned, and every call says why).
    pub runtime: Result<Runtime, String>,
    /// `sidecar/main.ts`.
    pub script: PathBuf,
    pub settings: PathBuf,
    /// Where the scanners' messages go in a release build (the console while Haunt is worked on).
    pub log: Option<PathBuf>,
    /// Where the scanners keep their call graphs between launches (the app's cache folder); none in the tests.
    pub cache: Option<PathBuf>,
    /// The app's log folder, for the Settings window; none while Haunt is worked on.
    pub logs: Option<PathBuf>,
}

#[derive(Clone, Default)]
struct Code {
    /// The last structure, as JSON text, and what Rust needs of it.
    structure: Option<String>,
    shape: Option<Shape>,
    /// The call graph for it, as the sidecar's whole message (JSON text).
    calls: Option<String>,
}

struct Shown {
    root: String,
    /// Changes with every new scanner process, so a stopped one's last words are ignored.
    generation: u64,
    sidecar: Option<Sidecar>,
    code: Code,
    // The watchers are held only to keep them running: dropping one stops it.
    #[allow(dead_code)]
    watch_root: Option<Watcher>,
    watch_folders: Vec<Watcher>,
    watching: Vec<String>,
    #[allow(dead_code)]
    watch_git: Vec<Watcher>,
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    watch_sidecar: Option<Watcher>,
}

/// Another tab: its last scan, and its scanner with the generation it was started as, while it still runs.
struct Kept {
    root: String,
    code: Code,
    scanner: Option<(Sidecar, u64)>,
}

struct Inner {
    settings: Settings,
    shown: Option<Shown>,
    /// The other tabs, the one shown last at the end.
    kept: Vec<Kept>,
    generation: u64,
}

pub struct Repos {
    context: Context,
    events: Arc<Events>,
    inner: Mutex<Inner>,
    /// The reader (sidecar/read.ts): one for the app, for the functions of a TypeScript-family file's other versions.
    reader: Mutex<Option<Sidecar>>,
    /// Per repository, the git parts Haunt announced after its own last write, and when: the .git watcher's news about
    /// the same write, a moment later, is not sent again (the window would read everything twice).
    announced: Mutex<HashMap<String, (Instant, Vec<String>)>>,
    me: Weak<Repos>,
}

/// How long after Haunt's own write the watcher's news about the same parts counts as that write's.
const ANNOUNCED_FOR: Duration = Duration::from_millis(1500);

impl Repos {
    pub fn new(context: Context, events: Arc<Events>) -> Arc<Repos> {
        let settings = Settings::load(&context.settings);
        Arc::new_cyclic(|me| Repos {
            context,
            events,
            inner: Mutex::new(Inner { settings, shown: None, kept: Vec::new(), generation: 0 }),
            reader: Mutex::new(None),
            announced: Mutex::default(),
            me: me.clone(),
        })
    }

    /// Shows the repository shown last time, if it can still be opened.
    pub fn start(&self) {
        let root = self.inner.lock().unwrap().settings.root.clone();
        if let Some(root) = root.filter(|root| paths::openable(Path::new(root))) {
            let mut inner = self.inner.lock().unwrap();
            self.show(&mut inner, &root);
        }
    }

    /// The repository shown, the tabs, the recent ones, its trust and what it would run if trusted.
    pub fn info(&self) -> RootInfo {
        let mut info = {
            let inner = self.inner.lock().unwrap();
            self.info_of(&inner)
        };
        // What the folder would run is found by starting git (its hooks): after the lock is let go, so no other call
        // waits for it.
        info.wants = info.root.as_deref().map(wants).unwrap_or_default();
        info
    }

    fn info_of(&self, inner: &Inner) -> RootInfo {
        let root = inner.shown.as_ref().map(|shown| shown.root.clone());
        let openable = |paths: &[String]| {
            paths.iter().filter(|path| paths::openable(Path::new(path))).cloned().collect::<Vec<_>>()
        };
        RootInfo {
            tabs: openable(&inner.settings.tabs),
            recent: openable(&inner.settings.recent),
            trusted: root.as_deref().is_some_and(|root| inner.settings.trusts(root)),
            wants: Vec::new(),
            root,
            worktrees: inner
                .settings
                .tabs
                .iter()
                .filter(|tab| Path::new(tab).join(".git").is_file())
                .cloned()
                .collect(),
            profile: inner.settings.profile.clone(),
            profiles: inner.settings.profile_names(),
            appearance: inner.settings.appearance.clone(),
            tab_looks: inner
                .settings
                .tab_looks
                .iter()
                .filter(|(path, _)| inner.settings.tabs.contains(path))
                .map(|(path, look)| (path.clone(), look.clone()))
                .collect(),
        }
    }

    /// Changes the profiles (`settings.rs`); when that changes the repository to show (another profile, or its tab
    /// moved to another one), shows it, or nothing.
    pub fn change_profiles(
        &self,
        change: impl FnOnce(&mut crate::settings::Settings) -> Result<(), String>,
    ) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock().unwrap();
            change(&mut inner.settings)?;
            let wanted = inner.settings.root.clone().filter(|root| paths::openable(Path::new(root)));
            let shown = inner.shown.as_ref().map(|shown| shown.root.clone());
            if wanted != shown {
                match wanted {
                    Some(root) => self.show(&mut inner, &root),
                    None => {
                        inner.shown = None;
                        self.events.send(None, json!({ "type": "root", "root": null }));
                    }
                }
            }
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// The git settings, the trusted folders and Haunt's own folders, for the Settings window.
    pub fn app_settings(&self) -> AppSettings {
        let inner = self.inner.lock().unwrap();
        let text = |path: &Path| path.to_string_lossy().into_owned();
        AppSettings {
            pull: inner.settings.pull,
            fetch_every: inner.settings.fetch_every,
            host_tools: inner.settings.host_tools,
            update_check: inner.settings.update_check,
            trusted: inner
                .settings
                .trusted
                .iter()
                .map(|path| TrustedFolder { path: path.clone(), missing: !Path::new(path).is_dir() })
                .collect(),
            version: env!("CARGO_PKG_VERSION").into(),
            folders: AppFolders {
                settings: self.context.settings.parent().map(text).unwrap_or_default(),
                cache: self.context.cache.as_deref().map(text),
                logs: self.context.logs.as_deref().map(text),
            },
        }
    }

    /// What a pull does when the branches went apart, and how often to fetch in the background (0, 5, 15 or 30).
    pub fn set_git(&self, pull: PullChoice, fetch_every: u32) -> Result<AppSettings, String> {
        if !FETCH_EVERY.contains(&fetch_every) {
            return Err(format!("Fetching every {fetch_every} minutes is not one of the choices."));
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.settings.pull = pull;
            inner.settings.fetch_every = fetch_every;
            inner.settings.save(&self.context.settings);
        }
        Ok(self.app_settings())
    }

    /// The switches for what Haunt may ask outside services: pull request status through the host's tool, and the update
    /// check.
    pub fn set_online(&self, host_tools: bool, update_check: bool) -> AppSettings {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.settings.host_tools = host_tools;
            inner.settings.update_check = update_check;
            inner.settings.save(&self.context.settings);
        }
        self.app_settings()
    }

    pub fn host_tools(&self) -> bool {
        self.inner.lock().unwrap().settings.host_tools
    }

    /// Stops trusting a folder, shown or not. A tab of it that keeps its scanner running loses the scanner, which ran
    /// the folder's own code; the next time it is shown it is scanned as untrusted.
    pub fn forget_trust(&self, path: &str) -> AppSettings {
        let shown = self.inner.lock().unwrap().shown.as_ref().is_some_and(|shown| shown.root == path);
        if shown {
            let _ = self.set_trust(path, false);
        } else {
            let mut inner = self.inner.lock().unwrap();
            inner.settings.set_trust(path, false);
            inner.settings.save(&self.context.settings);
            for kept in inner.kept.iter_mut().filter(|kept| kept.root == path) {
                kept.scanner = None;
            }
        }
        self.app_settings()
    }

    /// Removes the call graphs the scanners keep between launches; how many there were.
    pub fn clear_call_graphs(&self) -> Result<u32, String> {
        let Some(dir) = &self.context.cache else { return Ok(0) };
        let mut removed = 0;
        for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())?.flatten() {
            if entry.path().is_file() && std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// One of Haunt's own folders, for the file manager.
    pub fn folder(&self, which: AppFolder) -> Option<PathBuf> {
        match which {
            AppFolder::Settings => self.context.settings.parent().map(Path::to_path_buf),
            AppFolder::Cache => self.context.cache.clone(),
            AppFolder::Logs => self.context.logs.clone(),
        }
    }

    /// How often to fetch in the background (minutes, 0 off) and the repository shown, for the background fetch.
    pub fn fetch_schedule(&self) -> (u32, Option<String>) {
        let inner = self.inner.lock().unwrap();
        (inner.settings.fetch_every, inner.shown.as_ref().map(|shown| shown.root.clone()))
    }

    /// The shown repository's scanner, as the generation it was started as.
    #[cfg(test)]
    pub fn scanner(&self) -> Option<u64> {
        let inner = self.inner.lock().unwrap();
        inner.shown.as_ref().filter(|shown| shown.sidecar.is_some()).map(|shown| shown.generation)
    }

    /// Asks the reader for the functions of a TypeScript-family file's text (another version of `path` in `repo`). It
    /// scans nothing, so it answers at once even while the repository's sidecar works out a call graph; it is started
    /// when first needed, and again if it stopped.
    pub fn read_functions(&self, repo: &str, path: &str, text: &str) -> Result<Receiver<Reply>, String> {
        self.require(repo)?;
        let mut reader = self.reader.lock().unwrap();
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
            .map_err(|error| format!("{} could not start the reader: {error}", runtime.name()))?;
            *reader = Some(started);
        }
        let reader = reader.as_ref().expect("started");
        Ok(reader.ask(json!({ "type": "functions", "root": repo, "path": path, "text": text })))
    }

    /// Refuses a call for a repository that is not the one shown.
    pub fn require(&self, repo: &str) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        self.check(&inner, repo)
    }

    /// The app's config folder (where the settings are), for what Haunt keeps beside them.
    pub fn config_dir(&self) -> PathBuf {
        self.context.settings.parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// Node, for running the project's tests; or why it cannot be used. Looked for each time: Bun or Deno may run
    /// the scanner while Node is installed later.
    pub fn node(&self) -> Result<std::path::PathBuf, String> {
        crate::startup::find_node()
    }

    /// Whether the shown repository's own code may run (its hooks and filters, for writing); refused for another one.
    pub fn trusted(&self, repo: &str) -> Result<bool, String> {
        let inner = self.inner.lock().unwrap();
        self.check(&inner, repo)?;
        Ok(inner.settings.trusts(repo))
    }

    /// Sends the window news about the shown repository `repo`.
    pub fn news(&self, repo: &str, news: serde_json::Value) {
        self.events.send(Some(repo), news);
    }

    /// Tells the window something it could not do outside a repository's work (a dropped folder it cannot open).
    pub fn notice(&self, message: &str) {
        self.events.send(None, json!({ "type": "notice", "message": message }));
    }

    /// Tells the window that Haunt changed `parts` of git itself (a write), whether or not a watcher notices.
    pub fn announce_git(&self, repo: &str, parts: &[&str]) {
        let names = parts.iter().map(|part| part.to_string()).collect();
        self.announced.lock().unwrap().insert(repo.to_owned(), (Instant::now(), names));
        self.events.send(Some(repo), json!({ "type": "git", "parts": parts }));
    }

    /// Whether the watcher's news about `parts` is Haunt's own last write, announced a moment ago already.
    fn just_announced(&self, repo: &str, parts: &[&str]) -> bool {
        self.announced.lock().unwrap().get(repo).is_some_and(|(at, announced)| {
            at.elapsed() < ANNOUNCED_FOR && parts.iter().all(|part| announced.iter().any(|known| known == part))
        })
    }

    /// Shows another repository, in its tab (a new tab if it has none), and remembers it.
    pub fn open(&self, path: &str) -> Result<RootInfo, String> {
        let root = paths::canonical(path).ok_or_else(|| format!("{path} is not a folder Haunt can find."))?;
        if !paths::openable(Path::new(&root)) {
            return Err(format!("{root} is not a project or a git repository."));
        }
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.shown.as_ref().map(|shown| shown.root.as_str()) != Some(root.as_str()) {
                self.show(&mut inner, &root);
            }
            inner.settings.show(&root);
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// Puts the tabs in a new order (the same tabs, moved).
    pub fn reorder(&self, tabs: &[String]) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock().unwrap();
            let mut wanted: Vec<&String> = tabs.iter().collect();
            let mut known: Vec<&String> = inner.settings.tabs.iter().collect();
            wanted.sort();
            known.sort();
            if wanted != known {
                return Err("The tabs changed meanwhile.".into());
            }
            inner.settings.tabs = tabs.to_vec();
            // A tab dragged among the pinned ones stays unpinned, after them.
            inner.settings.pinned_first();
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// An open tab's folder, or a recent one's, for the file manager; any other path is refused.
    pub fn tab_folder(&self, path: &str) -> Result<PathBuf, String> {
        let inner = self.inner.lock().unwrap();
        inner
            .settings
            .tabs
            .iter()
            .chain(inner.settings.recent.iter())
            .find(|tab| *tab == path)
            .map(PathBuf::from)
            .ok_or_else(|| "That is not an open tab or a recent repository.".into())
    }

    /// The recent repositories of the profile in use, with what the open screen shows of each (only these folders are
    /// read: what the settings already name).
    pub fn recent_details(&self) -> Vec<RecentRepo> {
        let recent = self.inner.lock().unwrap().settings.recent.clone();
        // Each costs a few git calls (its branch, status and last commit): read side by side, in the list's order.
        std::thread::scope(|scope| {
            let reading: Vec<_> = recent.iter().map(|path| scope.spawn(move || RecentRepo::read(path))).collect();
            reading
                .into_iter()
                .zip(&recent)
                .map(|(read, path)| read.join().unwrap_or_else(|_| RecentRepo::read(path)))
                .collect()
        })
    }

    /// Takes a folder off the recent list.
    pub fn forget_recent(&self, path: &str) -> RootInfo {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.settings.forget_recent(path);
            inner.settings.save(&self.context.settings);
        }
        self.info()
    }

    /// Closes a tab; closing the one shown shows the tab next to it, or none.
    pub fn close(&self, path: &str) -> RootInfo {
        let root = paths::canonical(path).unwrap_or_else(|| path.to_owned());
        {
            let mut inner = self.inner.lock().unwrap();
            inner.kept.retain(|kept| kept.root != root);
            crate::code::others::forget(&root);
            crate::git::forget_config(&root);
            match inner.settings.close(&root) {
                None => {}
                Some(next) => {
                    inner.shown = None;
                    inner.kept.retain(|kept| kept.root != root);
                    match next.filter(|next| paths::openable(Path::new(next))) {
                        Some(next) => self.show(&mut inner, &next),
                        None => self.events.send(None, json!({ "type": "root", "root": null })),
                    }
                }
            }
            inner.settings.save(&self.context.settings);
        }
        self.info()
    }

    /// Lets the shown repository's own code run, or stops it; the scanner starts again either way.
    pub fn set_trust(&self, repo: &str, trusted: bool) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock().unwrap();
            self.check(&inner, repo)?;
            inner.settings.set_trust(repo, trusted);
            inner.settings.save(&self.context.settings);
            self.restart(&mut inner, if trusted { "trusting this folder" } else { "no longer trusting this folder" });
        }
        self.events.send(Some(repo), json!({ "type": "trust", "trusted": trusted }));
        Ok(self.info())
    }

    pub fn rescan(&self, repo: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        self.check(&inner, repo)?;
        self.scan(&mut inner, "a rescan");
        Ok(())
    }

    /// The latest structure as JSON text, or None until the first scan is in. It never waits: a call held open would
    /// take one of the few connections the window has, and the `scan` event says when to ask again.
    pub fn structure(&self, repo: &str) -> Result<Option<String>, String> {
        let inner = self.inner.lock().unwrap();
        self.check(&inner, repo)?;
        Ok(inner.shown.as_ref().expect("checked").code.structure.clone())
    }

    /// The call graph of the latest structure (`scannedAt`, `calls`) as JSON text, or null until it is worked out.
    pub fn calls(&self, repo: &str) -> Result<String, String> {
        let inner = self.inner.lock().unwrap();
        self.check(&inner, repo)?;
        Ok(inner.shown.as_ref().and_then(|shown| shown.code.calls.clone()).unwrap_or_else(|| "null".into()))
    }

    /// Asks the shown repository's scanner something (`sidecar/serve.ts`), and waits for its answer: it answers after
    /// the first structure, between the parts of a scan.
    pub fn ask_scanner(&self, repo: &str, request: serde_json::Value) -> Result<String, String> {
        let asked = {
            let inner = self.inner.lock().unwrap();
            self.check(&inner, repo)?;
            let shown = inner.shown.as_ref().unwrap();
            if shown.code.structure.is_none() {
                return Err("The code is still being scanned: try again in a moment.".into());
            }
            match &shown.sidecar {
                Some(sidecar) => sidecar.ask(request),
                // Scanned here, without a runtime: answered here too.
                None if self.context.runtime.is_err() => {
                    return crate::code::scan::answer(&request, shown.code.structure.as_deref().unwrap_or("null"));
                }
                None => return Err("The scanner is not running.".into()),
            }
        };
        asked
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|_| "The scanner did not answer in time.".to_string())?
    }

    /// A file of the shown repository's last scan, as a path inside it; the editor and the source view take only these.
    pub fn scanned(&self, repo: &str, path: &str) -> Result<PathBuf, String> {
        let inner = self.inner.lock().unwrap();
        self.check(&inner, repo)?;
        let known = inner
            .shown
            .as_ref()
            .and_then(|shown| shown.code.shape.as_ref())
            .is_some_and(|shape| shape.files.iter().any(|file| file.path == path));
        if !known {
            return Err("That file is not in the scan.".into());
        }
        paths::inside(repo, path).ok_or_else(|| "That file is not in the repository.".into())
    }

    fn check(&self, inner: &Inner, repo: &str) -> Result<(), String> {
        match &inner.shown {
            Some(shown) if shown.root == repo => Ok(()),
            _ => Err("That repository is not the one shown.".into()),
        }
    }

    /// Shows `root`: the shown repository's scan is kept, `root`'s kept scan (if any) shows at once, and a scanner
    /// and watchers start for it.
    fn show(&self, inner: &mut Inner, root: &str) {
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
            let mut inner = repos.inner.lock().unwrap();
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
    fn restart(&self, inner: &mut Inner, reason: &str) {
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
                let problem = format!("{} could not start the scanner: {error}", runtime.name());
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
        let mut inner = self.inner.lock().unwrap();
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
                self.events.send(Some(root), json!({ "type": "error", "message": "The scanner stopped. Save a file or rescan to start it again." }));
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
                json!({ "type": "notice", "message": "The system's limit on watched folders was reached, so saving some files will not rescan. On Linux, raise it with: sudo sysctl fs.inotify.max_user_watches=524288" }),
            );
        }
    }
}

/// The folder git looks for hooks in (core.hooksPath, or the git folder's hooks), and the hooks there that git would run:
/// a file by a hook's name, executable where that counts.
fn hooks(root: &str) -> Vec<PathBuf> {
    let read = |args: &[&str]| {
        crate::git::cli::git(root, args).ok().map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
    };
    let folder = match read(&["config", "--get", "core.hooksPath"]) {
        Some(configured) => {
            let configured = PathBuf::from(configured);
            if configured.is_absolute() {
                configured
            } else {
                // Relative to the top of the working tree, as git takes it.
                read(&["rev-parse", "--show-toplevel"])
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(root))
                    .join(configured)
            }
        }
        None => match git_dir(root) {
            Some(dir) => dir.join("hooks"),
            None => return Vec::new(),
        },
    };
    HOOKS
        .iter()
        .map(|name| folder.join(name))
        .filter(|path| {
            let Ok(meta) = std::fs::metadata(path) else { return false };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && meta.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        })
        .collect()
}

/// A path for people: relative to the folder when inside it.
fn shown_path(root: &str, path: &Path) -> String {
    let plain = paths::plain(path.to_path_buf());
    let base = paths::plain(PathBuf::from(root));
    plain
        .strip_prefix(&base)
        .map(|rest| rest.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| plain.to_string_lossy().into_owned())
}

/// The code a folder would have Haunt run, found by looking at its files only: its own TypeScript, the architecture
/// script its haunt.config.json names, and its git hooks.
fn wants(root: &str) -> Vec<Wanted> {
    let mut out = Vec::new();
    let base = Path::new(root);
    if base.join("node_modules/typescript/package.json").exists() {
        out.push(Wanted {
            kind: WantedKind::Typescript,
            label: "own TypeScript".into(),
            path: "node_modules/typescript".into(),
        });
    }
    let script = std::fs::read_to_string(base.join("haunt.config.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|config| config["architecture"]["script"].as_str().map(str::to_owned));
    if let Some(script) = script {
        out.push(Wanted { kind: WantedKind::Script, label: "architecture script".into(), path: script });
    }
    for hook in hooks(root) {
        let name = hook.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Wanted { kind: WantedKind::Hook, label: format!("{name} hook"), path: shown_path(root, &hook) });
    }
    out
}

/// What each thing a folder would run says, to read before trusting it. A hook that only hands over to Husky's file
/// (`.husky/<hook>`) shows that file too.
pub fn wanted_texts(root: &str) -> Vec<WantedText> {
    let base = Path::new(root);
    let read = |path: &Path| -> (String, bool) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let truncated = bytes.len() > WANTED_TEXT;
                (String::from_utf8_lossy(&bytes[..bytes.len().min(WANTED_TEXT)]).into_owned(), truncated)
            }
            Err(_) => (String::new(), false),
        }
    };
    let mut out = Vec::new();
    for want in wants(root) {
        let file = if Path::new(&want.path).is_absolute() { PathBuf::from(&want.path) } else { base.join(&want.path) };
        match want.kind {
            WantedKind::Typescript => {
                let version = std::fs::read_to_string(file.join("package.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|package| package["version"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| "of an unknown version".into());
                out.push(WantedText {
                    label: want.label,
                    path: want.path,
                    text: String::new(),
                    note: format!("The TypeScript compiler installed in the folder ({version}), used to read its code with its own settings instead of Haunt's copy."),
                    truncated: false,
                });
            }
            WantedKind::Script => {
                let (text, truncated) = read(&file);
                out.push(WantedText {
                    label: want.label,
                    path: want.path,
                    note: if text.is_empty() {
                        "The file is not there.".into()
                    } else {
                        "Run by the scanner with Node, to sort the code into layers.".into()
                    },
                    text,
                    truncated,
                });
            }
            WantedKind::Hook => {
                let (text, truncated) = read(&file);
                let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                out.push(WantedText {
                    label: want.label.clone(),
                    path: want.path.clone(),
                    note: "Run by git when Haunt does what triggers it.".into(),
                    text: text.clone(),
                    truncated,
                });
                let husky = base.join(".husky").join(&name);
                if text.contains("husky") && husky.is_file() && husky != file {
                    let (text, truncated) = read(&husky);
                    out.push(WantedText {
                        label: format!("{name} (Husky)"),
                        path: shown_path(root, &husky),
                        note: "What the hook above hands over to.".into(),
                        text,
                        truncated,
                    });
                }
            }
        }
    }
    out
}

/// The repository's git folder (also for worktrees and submodules, whose .git is a file pointing elsewhere).
fn git_dir(root: &str) -> Option<PathBuf> {
    let found = crate::git::cli::git(root, &["rev-parse", "--absolute-git-dir"]).ok()?;
    let found = found.trim();
    (!found.is_empty()).then(|| paths::plain(PathBuf::from(found)))
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    fn executable(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn lists_the_hooks_git_would_run_with_their_text() {
        let r = crate::git::tests::Repo::new();
        let hooks = r.path(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        executable(&hooks.join("pre-commit"), "#!/bin/sh\nnpm test\n");
        executable(&hooks.join("pre-push.sample"), "#!/bin/sh\n");
        let found: Vec<(String, String)> = wants(&r.root).into_iter().map(|want| (want.label, want.path)).collect();
        assert_eq!(found, [("pre-commit hook".to_owned(), ".git/hooks/pre-commit".to_owned())], "samples do not run");
        let texts = wanted_texts(&r.root);
        assert!(texts[0].text.contains("npm test"));

        // Husky: core.hooksPath points at .husky/_, whose hook hands over to .husky/pre-push.
        std::fs::create_dir_all(r.path(".husky/_")).unwrap();
        executable(&r.path(".husky/_/pre-push"), "#!/usr/bin/env sh\n. \"$(dirname \"$0\")/husky.sh\"\n");
        std::fs::write(r.path(".husky/pre-push"), "pnpm lint\n").unwrap();
        r.sh(&["config", "core.hooksPath", ".husky/_"]);
        let texts = wanted_texts(&r.root);
        let labels: Vec<&str> = texts.iter().map(|text| text.label.as_str()).collect();
        assert_eq!(labels, ["pre-push hook", "pre-push (Husky)"]);
        assert_eq!(texts[0].path, ".husky/_/pre-push");
        assert!(texts[1].text.contains("pnpm lint"));
    }

    #[test]
    fn checks_a_typed_folder_and_reads_a_recent_repository() {
        let r = crate::git::tests::Repo::new();
        r.write("a.ts", "export const a = 1\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        r.write("b.ts", "export const b = 1\n");
        let found = check_folder(&r.root);
        assert!(found.exists && found.openable && found.repository);
        let missing = check_folder(&format!("{}/nowhere", r.root));
        assert!(!missing.exists && !missing.openable);
        let recent = RecentRepo::read(&r.root);
        assert_eq!((recent.branch.as_deref(), recent.changes), (Some("main"), Some(1)));
        assert_eq!(recent.last_commit.unwrap().subject, "First");
    }
}
