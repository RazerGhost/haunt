//! Repositories whose folder is missing: moved, renamed, deleted, or on a drive that is not connected. Their tabs and
//! recent entries are kept (settings.rs) and shown as missing; a look every few seconds tells when the shown one goes
//! (then it is no longer shown, and the window says so) and when one comes back (then its tab works again, and the one
//! shown last is shown again). Locate moves a repository's tab, look, trust and recent entry to where its folder is now.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::{Repos, RootInfo};
use crate::paths;

/// How often the folders of the tabs and recent repositories are looked at: a few file checks each.
const LOOK_EVERY: Duration = Duration::from_secs(3);
/// A repository whose folder is missing this long is forgotten: its tab, recent entry, look and saved call graph, and
/// what the window keeps of it. Its trust stays listed in Settings, Trust.
pub const FORGET_AFTER: u64 = 90 * 24 * 60 * 60;
/// A saved call graph of a repository Haunt no longer lists (no tab, not recent) is removed once it is this old.
const KEEP_UNLISTED: Duration = Duration::from_secs(30 * 24 * 60 * 60);

fn now_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_secs()).unwrap_or(0)
}

impl Repos {
    /// Looks at the folders now and then, for as long as Haunt runs; started once, however Haunt starts.
    pub fn look_for_missing(&self) {
        *self.missing.lock() = self.missing_now();
        let me = self.me.clone();
        std::thread::spawn(move || {
            if let Some(repos) = me.upgrade() {
                repos.remove_unlisted_call_graphs();
            }
            loop {
                std::thread::sleep(LOOK_EVERY);
                let Some(repos) = me.upgrade() else { return };
                repos.check_folders();
            }
        });
    }

    /// Where the scanner saves `root`'s call graph between launches.
    fn call_graph_file(&self, root: &str) -> Option<std::path::PathBuf> {
        Some(self.context.cache.as_ref()?.join(call_graph_name(root)))
    }

    fn remove_call_graph(&self, root: &str) {
        if let Some(file) = self.call_graph_file(root) {
            let _ = std::fs::remove_file(file);
        }
    }

    /// Saved call graphs of repositories Haunt no longer lists, untouched for a while, are removed (at start).
    fn remove_unlisted_call_graphs(&self) {
        let Some(dir) = self.context.cache.clone() else { return };
        let listed: Vec<std::path::PathBuf> = {
            let inner = self.inner.lock();
            let settings = &inner.settings;
            let mut all: Vec<&String> = settings.tabs.iter().chain(&settings.recent).chain(&settings.root).collect();
            for profile in &settings.profiles {
                all.extend(profile.tabs.iter().chain(&profile.recent).chain(&profile.root));
            }
            all.into_iter().filter_map(|path| self.call_graph_file(path)).collect()
        };
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let old = entry.metadata().and_then(|meta| meta.modified()).ok().and_then(|at| at.elapsed().ok());
            if path.extension().is_some_and(|ext| ext == "json")
                && !listed.contains(&path)
                && old.is_some_and(|old| old >= KEEP_UNLISTED)
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// Takes a repository away for good (Remove, on a missing one): its tab (showing the next one when it was shown),
    /// its recent entries in every profile, its look and its saved call graph. The window forgets its own data.
    pub fn forget_repo(&self, path: &str) -> RootInfo {
        let in_tab = self.inner.lock().settings.tabs.iter().any(|tab| tab == path);
        if in_tab {
            self.close(path);
        }
        {
            let mut inner = self.inner.lock();
            inner.settings.forget_repo(path, false);
            inner.settings.save(&self.context.settings);
        }
        self.remove_call_graph(path);
        *self.missing.lock() = self.missing_now();
        self.info()
    }

    /// The repositories forgotten since the window last asked, for it to forget what it keeps of them.
    pub fn take_forgotten(&self) -> Vec<String> {
        let mut inner = self.inner.lock();
        let forgotten = std::mem::take(&mut inner.settings.forgotten);
        if !forgotten.is_empty() {
            inner.settings.save(&self.context.settings);
        }
        forgotten
    }

    /// The tabs and recent repositories whose folders are missing now, in the order of the tabs, then the recent ones.
    pub(super) fn missing_now(&self) -> Vec<String> {
        let inner = self.inner.lock();
        let mut missing: Vec<String> = Vec::new();
        for path in inner.settings.tabs.iter().chain(&inner.settings.recent) {
            if !missing.contains(path) && !paths::openable(Path::new(path)) {
                missing.push(path.clone());
            }
        }
        missing
    }

    /// The shown repository's folder went: it is no longer shown (its watchers and scanner stop), and the window hears
    /// which one. A folder that came back: the window hears, and the one shown last is shown again when none is.
    pub fn check_folders(&self) {
        let mut missing = self.missing_now();
        // When each went missing; those missing for long are forgotten.
        let expired = {
            let mut inner = self.inner.lock();
            let now = now_seconds();
            let changed = inner.settings.note_missing(&missing, now, |path| paths::openable(Path::new(path)));
            let expired = inner.settings.long_missing(now, FORGET_AFTER);
            for path in &expired {
                inner.settings.forget_repo(path, true);
            }
            if changed || !expired.is_empty() {
                inner.settings.save(&self.context.settings);
            }
            expired
        };
        if !expired.is_empty() {
            for path in &expired {
                self.remove_call_graph(path);
            }
            missing.retain(|path| !expired.contains(path));
            self.events.send(None, json!({ "type": "forgotten" }));
        }
        let before = std::mem::replace(&mut *self.missing.lock(), missing.clone());
        let mut inner = self.inner.lock();
        let gone = inner.shown.as_ref().map(|shown| shown.root.clone()).filter(|root| missing.contains(root));
        if let Some(root) = gone {
            inner.shown = None;
            inner.kept.retain(|kept| kept.root != root);
            drop(inner);
            self.events.send(None, json!({ "type": "root", "root": null }));
            self.events.send(None, json!({ "type": "missing", "root": root }));
            return;
        }
        if missing == before && expired.is_empty() {
            return;
        }
        // Back: the repository shown last, when nothing is shown now.
        let back = inner
            .settings
            .root
            .clone()
            .filter(|root| inner.shown.is_none() && before.contains(root) && !missing.contains(root));
        if let Some(root) = back {
            self.show(&mut inner, &root);
            drop(inner);
            self.events.send(None, json!({ "type": "root", "root": root }));
            return;
        }
        drop(inner);
        self.events.send(None, json!({ "type": "tabs" }));
    }

    /// The repository at `old`, whose folder is missing, is at `path` now: its tab (in its place), look, trust and
    /// recent entry move there, the links of its worktrees are written again, and it is shown.
    pub fn relocate(&self, old: &str, path: &str) -> Result<RootInfo, String> {
        let new = paths::canonical(path)
            .ok_or_else(|| said!("repos.notFound", "{path} is not a folder Haunt can find.", path = path))?;
        if !paths::openable(Path::new(&new)) {
            return Err(said!("repos.notProject", "{root} is not a project or a git repository.", root = new));
        }
        if paths::openable(Path::new(old)) && new != old {
            return Err(said!("repos.notMissing", "{path} is there: it did not move.", path = old));
        }
        {
            let mut inner = self.inner.lock();
            inner.settings.relocate(old, &new);
            inner.settings.save(&self.context.settings);
        }
        // Its saved call graph goes with it (its files did not change, so it still holds).
        if let (Some(from), Some(to)) = (self.call_graph_file(old), self.call_graph_file(&new)) {
            if from.exists() && !to.exists() {
                let _ = std::fs::rename(from, to);
            }
        }
        repair_worktrees(&new);
        *self.missing.lock() = self.missing_now();
        self.open(&new)
    }
}

/// The file a repository's call graph is saved in: named as sidecar/cache.ts names it, by the hash of its path.
fn call_graph_name(root: &str) -> String {
    use sha2::{Digest, Sha256};
    let hex: String = Sha256::digest(root.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{}.json", &hex[..24])
}

/// A repository moved: its worktrees' `.git` files still lead to where it was. Written again (`git worktree repair`);
/// a git too old for it, or a worktree that moved too, is left as it is (the worktree list offers Repair and Locate).
fn repair_worktrees(root: &str) {
    let Ok(list) = crate::git::write::worktrees(root) else { return };
    let broken: Vec<String> =
        list.into_iter().filter(|worktree| worktree.broken).map(|worktree| worktree.path).collect();
    if !broken.is_empty() {
        let _ = crate::git::write::Writer::new(root, false).repair_worktrees(&broken);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_a_call_graph_file_as_the_scanner_does() {
        // sidecar/cache.ts: sha256 of the path, its first 24 hex digits.
        assert_eq!(super::call_graph_name(r"C:\Projects\app"), "97a656523e18c933a38d9dce.json");
    }
}
