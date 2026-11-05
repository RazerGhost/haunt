//! Undoing: under the rules of `write/mod.rs`.
//!
//! - Revert a commit (a new commit that takes its changes back) and cherry-pick one (its changes again, as a new commit
//!   on the branch you are on). A merge commit is taken against its first parent. Either may stop on conflicts, which
//!   are then resolved in the conflict view (`conflicts.rs`).
//! - Reset the current branch to a commit: soft (its changes stay staged), mixed (they stay in your files) or hard
//!   (they go). A hard reset saves your local changes first, as a discard does, so Recently discarded brings them back.
//! - Undo the last commit: its changes go back to staged, ready to commit again.
//! - Each of these writes files, so in an untrusted folder whose files go through filters (Git LFS) they are refused, as
//!   switching branches is.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::conflicts::OperationKind;
use super::{queue, Writer};
use crate::git::cli::{git as read, revision};

/// How far a reset goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ResetMode {
    /// The branch moves; the changes of the commits left behind stay staged.
    Soft,
    /// The branch moves; those changes stay in your files, unstaged.
    Mixed,
    /// The branch and your files move; your local changes are saved first.
    Hard,
}

/// What a revert, cherry-pick, merge or continue did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Done,
    /// Nothing to do: the branch has it already.
    UpToDate,
    /// Stopped on conflicts: resolve them in the conflict view, then continue (or abort).
    Conflicts,
}

impl Writer<'_> {
    pub(super) fn commit_of(&self, at: &str) -> Result<String, String> {
        let at = revision(at)?;
        let sha =
            read(self.root, &["rev-parse", "--verify", "--quiet", "--end-of-options", &format!("{at}^{{commit}}")])
                .map_err(|_| said!("undo.notCommit", "\"{at}\" is not a commit here.", at = at))?;
        Ok(sha.trim().to_owned())
    }

    fn is_merge(&self, sha: &str) -> bool {
        read(self.root, &["rev-list", "--parents", "-n", "1", sha])
            .is_ok_and(|line| line.split_whitespace().count() > 2)
    }

    /// Refuses while a merge, revert, cherry-pick or rebase is going on: that one is finished or aborted first.
    pub(super) fn refuse_busy(&self) -> Result<(), String> {
        match self.operation()? {
            Some(going) => Err(match going.kind {
                OperationKind::Merge => said!("undo.busyMerge", "A merge is not finished. Continue or abort it first."),
                OperationKind::Revert => {
                    said!("undo.busyRevert", "A revert is not finished. Continue or abort it first.")
                }
                OperationKind::CherryPick => {
                    said!("undo.busyCherryPick", "A cherry-pick is not finished. Continue or abort it first.")
                }
                OperationKind::Rebase => {
                    said!("undo.busyRebase", "A rebase is not finished. Continue or abort it first.")
                }
            }),
            None => Ok(()),
        }
    }

    /// A new commit on the current branch that takes `at`'s changes back.
    pub fn revert(&self, at: &str) -> Result<Outcome, String> {
        self.refuse_checkout(super::refs::Checkout::Revert)?;
        self.refuse_busy()?;
        let sha = self.commit_of(at)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let mut args = vec!["revert", "--no-edit"];
        if self.is_merge(&sha) {
            args.extend(["-m", "1"]);
        }
        args.extend(["--end-of-options", &sha]);
        self.apply(&args, "REVERT_HEAD")
    }

    /// `at`'s changes again, as a new commit on the current branch.
    pub fn cherry_pick(&self, at: &str) -> Result<Outcome, String> {
        self.refuse_checkout(super::refs::Checkout::CherryPick)?;
        self.refuse_busy()?;
        let sha = self.commit_of(at)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let mut args = vec!["cherry-pick"];
        if self.is_merge(&sha) {
            args.extend(["-m", "1"]);
        }
        args.extend(["--end-of-options", &sha]);
        self.apply(&args, "CHERRY_PICK_HEAD")
    }

    /// Runs a revert or cherry-pick: done, or stopped on conflicts (the marker `head` is there), or, when the change is
    /// on the branch already, undone and said so.
    fn apply(&self, args: &[&str], head: &str) -> Result<Outcome, String> {
        match self.git(args, None, &[], None) {
            Ok(_) => Ok(Outcome::Done),
            Err(said) if said.contains("now empty") || said.contains("nothing to commit") => {
                let abort = if head == "REVERT_HEAD" { "revert" } else { "cherry-pick" };
                if self.git_path_exists(head) {
                    let _ = self.git(&[abort, "--abort"], None, &[], None);
                }
                Ok(Outcome::UpToDate)
            }
            Err(said) if self.git_path_exists(head) => {
                if self.conflicted()?.is_empty() {
                    Err(said)
                } else {
                    Ok(Outcome::Conflicts)
                }
            }
            Err(said) => Err(said),
        }
    }

    /// Moves the current branch to `at`. A hard reset saves your local changes first (Recently discarded).
    pub fn reset(&self, at: &str, mode: ResetMode) -> Result<(), String> {
        if mode == ResetMode::Hard {
            self.refuse_checkout(super::refs::Checkout::HardReset)?;
        }
        self.refuse_busy()?;
        let sha = self.commit_of(at)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        if mode == ResetMode::Hard {
            let changed = self.changed_tracked()?;
            let paths: Vec<&str> = changed.iter().map(String::as_str).collect();
            if !paths.is_empty() {
                self.save(&paths, "Saved before a hard reset")?;
            }
        }
        let flag = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        self.git(&["reset", "-q", flag, &sha, "--"], None, &[], None).map(drop)
    }

    /// The tracked files with changes, staged or not (what a hard reset would throw away), relative to the folder.
    fn changed_tracked(&self) -> Result<Vec<String>, String> {
        let Some(status) = crate::git::local::status(self.root)? else { return Ok(Vec::new()) };
        let mut paths: Vec<String> = status
            .staged
            .iter()
            .chain(status.unstaged.iter())
            .filter(|file| !file.untracked)
            .flat_map(|file| [file.path.clone(), file.old_path.clone()])
            .filter(|path| !path.is_empty())
            .collect();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Takes the last commit back: the branch goes to its parent and its changes are staged again.
    pub fn undo_commit(&self) -> Result<(), String> {
        self.refuse_busy()?;
        if read(self.root, &["rev-parse", "--verify", "--quiet", "HEAD~1^{commit}"]).is_err() {
            return Err(said!("undo.firstCommit", "The first commit of a repository cannot be undone this way."));
        }
        if self.is_merge("HEAD") {
            return Err(said!(
                "undo.merge",
                "The last commit is a merge. Reset the branch to the commit you want instead."
            ));
        }
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        self.git(&["reset", "-q", "--soft", "HEAD~1"], None, &[], None).map(drop)
    }

    /// The remote branches that have `at`: undoing or resetting past it changes history others may have.
    pub fn on_remotes(&self, at: &str) -> Result<Vec<String>, String> {
        let sha = self.commit_of(at)?;
        let out = read(self.root, &["branch", "-r", "--format=%(refname:short)", "--contains", &sha])?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.ends_with("/HEAD"))
            .map(str::to_owned)
            .collect())
    }
}
