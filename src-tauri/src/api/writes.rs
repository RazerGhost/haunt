//! How a command writes: through `Writer` (git/write), as the repository's trust allows, and what the window hears after.

use super::Api;
use crate::git::local::LocalStatus;
use crate::git::write::{Undo, Writer};

impl Api {
    /// Runs a write in the shown repository `repo`, as its trust allows, and tells the window which parts of git changed.
    pub(super) fn write<T>(
        &self,
        repo: &str,
        parts: &[&str],
        write: impl FnOnce(&Writer) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        crate::git::write::forget_last(repo);
        let result = write(&Writer::new(repo, trusted));
        // A failed write may have done part of its work (a save before a discard): the window reads again either way.
        self.repos.announce_git(repo, parts);
        result
    }

    /// A write that moves history, noted so the window can offer to undo it (git/write/last.rs): `title` names what
    /// it did, from its result.
    pub(super) fn undoable<T>(
        &self,
        repo: &str,
        parts: &[&str],
        undo: Undo,
        title: impl FnOnce(&T) -> String,
        write: impl FnOnce(&Writer) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        let writer = Writer::new(repo, trusted);
        let before = writer.snapshot();
        crate::git::write::forget_last(repo);
        let result = write(&writer);
        if let Ok(value) = &result {
            self.noted(repo, &writer, title(value), before, undo);
        }
        self.repos.announce_git(repo, parts);
        result
    }

    /// Notes a write that went through and tells the window what Undo would do.
    pub(super) fn noted(
        &self,
        repo: &str,
        writer: &Writer,
        title: String,
        before: crate::git::write::Snapshot,
        undo: Undo,
    ) {
        if let Some(last) = writer.record(title, before, undo) {
            self.repos.news(repo, serde_json::json!({ "type": "done", "title": last.title, "undo": last.undo }));
        }
    }

    /// A fetch, pull or push in `repo`: the window hears what `op` is doing (git's progress) and when it is done.
    pub(super) fn network<T>(
        &self,
        repo: &str,
        op: &str,
        parts: &[&str],
        run: impl FnOnce(&Writer, &dyn Fn(&str)) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        crate::git::write::forget_last(repo);
        self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "text": "" }));
        let progress =
            |text: &str| self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "text": text }));
        let result = run(&Writer::new(repo, trusted), &progress);
        self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "done": true }));
        self.repos.announce_git(repo, parts);
        result
    }

    /// A remote's write in a repository by its path; a tab that shows it reads its refs again.
    pub(super) fn remote_by_path(
        &self,
        path: &str,
        write: impl FnOnce(&Writer) -> Result<(), String>,
    ) -> Result<(), String> {
        let root = crate::git::overview::repository(path)?;
        let result = write(&Writer::new(&root, false));
        self.repos.announce_git(&root, &["refs"]);
        result
    }

    /// A write that changes the lists of local changes: the lists after it.
    pub(super) fn write_listed(
        &self,
        repo: &str,
        write: impl FnOnce(&Writer) -> Result<(), String>,
    ) -> Result<Option<LocalStatus>, String> {
        self.write(repo, &["index"], write)?;
        crate::git::local::status(repo)
    }
}
