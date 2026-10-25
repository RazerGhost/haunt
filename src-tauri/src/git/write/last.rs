//! Undo for the last write that moved history: a commit or amend, a merge, pull, revert, cherry-pick, reset or rebase,
//! switching branches, and creating, renaming or deleting a branch. Before such a write the branch and commit are
//! noted; after it, what Undo would do. Undo puts them back, and only while nothing has moved since: any other write
//! (or a change made outside Haunt) leaves nothing to undo. One step back, no redo; kept while the app runs.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use specta::Type;

use super::{queue, SwitchMode, Writer};
use crate::git::cli::git as read;

/// Where HEAD is: its branch (none when detached) and commit (none before the first commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    branch: Option<String>,
    sha: Option<String>,
}

/// How a write is undone.
#[derive(Debug, Clone)]
pub enum Undo {
    /// The branch goes back to where it was: `--soft` keeps the changes staged (a commit, an amend, a soft reset),
    /// `--mixed` in the files (a mixed reset), `--keep` moves the files too, refusing to overwrite local changes.
    Reset(&'static str),
    /// Back to the branch that was checked out.
    SwitchBack,
    /// A deleted branch, made again where it was.
    Recreate { name: String, sha: String },
    /// A new branch, removed (after switching back, when the write switched to it).
    RemoveCreated { name: String },
    /// A renamed branch, named as it was.
    RenameBack { from: String, to: String },
}

struct Done {
    title: String,
    before: Snapshot,
    after: Snapshot,
    undo: Undo,
}

/// The last write that can be undone, and what Undo does.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LastAction {
    pub title: String,
    pub undo: String,
}

fn done() -> &'static Mutex<HashMap<String, Done>> {
    static DONE: OnceLock<Mutex<HashMap<String, Done>>> = OnceLock::new();
    DONE.get_or_init(Default::default)
}

/// Another write: what was done before it can no longer be undone.
pub fn forget_last(root: &str) {
    done().lock().unwrap().remove(root);
}

impl Done {
    fn sentence(&self) -> String {
        let short =
            |sha: &Option<String>| sha.as_deref().map_or("".to_owned(), |sha| sha[..sha.len().min(7)].to_owned());
        let branch = self.before.branch.as_deref().unwrap_or("HEAD");
        match &self.undo {
            Undo::Reset("--soft") => {
                format!("{branch} goes back to {}; the changes stay staged.", short(&self.before.sha))
            }
            Undo::Reset("--mixed") => {
                format!("{branch} goes back to {}; your files stay as they are.", short(&self.before.sha))
            }
            Undo::Reset(_) => format!("{branch} and its files go back to {}.", short(&self.before.sha)),
            Undo::SwitchBack => format!("Back to {branch}."),
            Undo::Recreate { name, sha } => format!("{name} is made again at {}.", &sha[..sha.len().min(7)]),
            Undo::RemoveCreated { name } if self.before.branch != self.after.branch => {
                format!("Back to {branch}, and {name} is deleted.")
            }
            Undo::RemoveCreated { name } => format!("{name} is deleted."),
            Undo::RenameBack { from, to } => format!("{to} is named {from} again."),
        }
    }
}

impl Writer<'_> {
    pub fn snapshot(&self) -> Snapshot {
        let line =
            |args: &[&str]| read(self.root, args).ok().map(|out| out.trim().to_owned()).filter(|out| !out.is_empty());
        Snapshot {
            branch: line(&["symbolic-ref", "--short", "-q", "HEAD"]),
            sha: line(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]),
        }
    }

    /// Notes a write that went through, so it can be undone; returns what Undo would do. Nothing is noted when the
    /// write stopped half way (on conflicts), or changed nothing Undo could put back.
    pub fn record(&self, title: String, before: Snapshot, undo: Undo) -> Option<LastAction> {
        forget_last(self.root);
        if !matches!(self.operation(), Ok(None)) {
            return None;
        }
        let after = self.snapshot();
        let usable = match &undo {
            Undo::Reset(_) => before.sha.is_some() && before.branch == after.branch && before.sha != after.sha,
            Undo::SwitchBack => before.branch.is_some() && before.branch != after.branch,
            Undo::Recreate { .. } | Undo::RemoveCreated { .. } | Undo::RenameBack { .. } => true,
        };
        if !usable {
            return None;
        }
        let entry = Done { title, before, after, undo };
        let last = LastAction { title: entry.title.clone(), undo: entry.sentence() };
        done().lock().unwrap().insert(self.root.to_owned(), entry);
        Some(last)
    }

    /// What can be undone now, if anything.
    pub fn last_action(&self) -> Option<LastAction> {
        let done = done().lock().unwrap();
        let entry = done.get(self.root)?;
        (self.snapshot() == entry.after).then(|| LastAction { title: entry.title.clone(), undo: entry.sentence() })
    }

    /// Undoes the last write; returns its title.
    pub fn undo_last(&self) -> Result<String, String> {
        let entry = done().lock().unwrap().remove(self.root).ok_or("There is nothing to undo.")?;
        if self.snapshot() != entry.after {
            return Err("Something has changed since, so it cannot be undone from here.".into());
        }
        let back = |name: &Option<String>| -> Result<(), String> {
            match name {
                Some(name) => self.switch_branch(name, SwitchMode::Carry),
                None => Err("HEAD was not on a branch before.".into()),
            }
        };
        match &entry.undo {
            Undo::Reset(flag) => {
                let sha = entry.before.sha.as_deref().ok_or("There was no commit before.")?;
                if *flag == "--keep" {
                    self.refuse_checkout("Undoing")?;
                }
                let queue = queue(self.root);
                let _turn = queue.lock().unwrap();
                self.wait_for_lock()?;
                self.git(&["reset", "-q", flag, sha, "--"], None, &[], None).map_err(|said| {
                    if said.contains("not uptodate") || said.contains("would be overwritten") {
                        "Your local changes are in the files it would change. Commit, stash or discard them, then undo."
                            .to_owned()
                    } else {
                        said
                    }
                })?;
            }
            Undo::SwitchBack => back(&entry.before.branch)?,
            Undo::Recreate { name, sha } => self.create_branch(name, Some(sha), false)?,
            Undo::RemoveCreated { name } => {
                if entry.before.branch != entry.after.branch {
                    back(&entry.before.branch)?;
                }
                self.delete_branch(name, true)?;
            }
            Undo::RenameBack { from, to } => self.rename_branch(to, from)?,
        }
        Ok(entry.title)
    }
}
