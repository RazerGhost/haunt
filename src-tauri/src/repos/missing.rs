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

impl Repos {
    /// Looks at the folders now and then, for as long as Haunt runs; started once, however Haunt starts.
    pub fn look_for_missing(&self) {
        *self.missing.lock() = self.missing_now();
        let me = self.me.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(LOOK_EVERY);
            let Some(repos) = me.upgrade() else { return };
            repos.check_folders();
        });
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
        let missing = self.missing_now();
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
        if missing == before {
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
        repair_worktrees(&new);
        *self.missing.lock() = self.missing_now();
        self.open(&new)
    }
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
