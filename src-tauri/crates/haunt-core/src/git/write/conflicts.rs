//! Merging and conflicts: under the rules of `write/mod.rs`.
//!
//! - Merge a branch into the current one (a fast-forward when it can be).
//! - A merge, revert, cherry-pick, pull or rebase that stops on conflicts leaves them in place: `operation` says what is
//!   going on and which files conflict, `conflict` gives a file's three versions (the common base, yours, theirs) and
//!   the file as it is now, with git's markers. Each file is resolved by taking one side, or by saving the result
//!   written in the window; it is staged then. Continue finishes the operation once no file conflicts; abort puts
//!   everything back as it was before it.
//! - In a rebase git's "ours" is the branch rebased onto and "theirs" your commit; the labels say so.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{queue, Outcome, Writer};
use crate::git::cli::{git as read, git_bytes};
use crate::git::local::checked;

/// What is going on in the repository that stopped for you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Merge,
    Revert,
    CherryPick,
    Rebase,
}

/// A merge, revert, cherry-pick or rebase that has not finished.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Operation {
    pub kind: OperationKind,
    /// "Merging feature/login into main".
    pub title: String,
    /// What "yours" and "theirs" are here.
    pub ours: String,
    pub theirs: String,
    /// The files that still conflict, relative to the folder.
    pub conflicted: Vec<String>,
}

/// A conflicted file: its common base, yours and theirs (null where that side has no such file), and the file as it is
/// now, with git's conflict markers.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ConflictFile {
    pub path: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub result: Option<String>,
    /// A binary file: only taking one side works.
    pub binary: bool,
}

/// One side of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Ours,
    Theirs,
}

impl Writer<'_> {
    /// The files that still conflict, relative to the folder.
    pub(super) fn conflicted(&self) -> Result<Vec<String>, String> {
        let out = read(self.root, &["diff", "--name-only", "--relative", "--diff-filter=U", "-z"])?;
        let mut paths: Vec<String> = out.split('\0').filter(|path| !path.is_empty()).map(str::to_owned).collect();
        paths.dedup();
        Ok(paths)
    }

    fn subject_of(&self, at: &str) -> String {
        read(self.root, &["log", "-1", "--format=%s", at, "--"]).map(|out| out.trim().to_owned()).unwrap_or_default()
    }

    fn read_git_file(&self, name: &str) -> Option<String> {
        let path = read(self.root, &["rev-parse", "--git-path", name]).ok()?;
        std::fs::read_to_string(std::path::Path::new(self.root).join(path.trim())).ok()
    }

    /// The merge, revert, cherry-pick or rebase going on, if any.
    pub fn operation(&self) -> Result<Option<Operation>, String> {
        let branch = self.current_branch().unwrap_or_else(|_| "HEAD".into());
        let (kind, title, ours, theirs) = if self.git_path_exists("rebase-merge")
            || self.git_path_exists("rebase-apply")
        {
            let name = self
                .read_git_file("rebase-merge/head-name")
                .or_else(|| self.read_git_file("rebase-apply/head-name"))
                .map(|name| name.trim().trim_start_matches("refs/heads/").to_owned());
            let onto = self.read_git_file("rebase-merge/onto").or_else(|| self.read_git_file("rebase-apply/onto")).map(
                |sha| {
                    let sha = sha.trim().to_owned();
                    read(self.root, &["name-rev", "--name-only", "--no-undefined", &sha])
                        .map(|name| name.trim().to_owned())
                        .unwrap_or_else(|_| sha.chars().take(7).collect())
                },
            );
            let title = match (&name, &onto) {
                (Some(name), Some(onto)) => {
                    said!("conflicts.rebasing", "Rebasing {name} onto {onto}", name = name, onto = onto)
                }
                (None, Some(onto)) => said!("conflicts.rebasingYours", "Rebasing your branch onto {onto}", onto = onto),
                (Some(name), None) => {
                    said!("conflicts.rebasingOntoOther", "Rebasing {name} onto the other branch", name = name)
                }
                (None, None) => said!("conflicts.rebasingYoursOntoOther", "Rebasing your branch onto the other branch"),
            };
            let ours = onto.unwrap_or_else(|| said!("conflicts.otherBranch", "the other branch"));
            let theirs = match name {
                Some(name) => said!("conflicts.yourCommitOn", "your commit on {name}", name = name),
                None => said!("conflicts.yourCommit", "your commit"),
            };
            (OperationKind::Rebase, title, ours, theirs)
        } else if self.git_path_exists("MERGE_HEAD") {
            let message = self.read_git_file("MERGE_MSG").unwrap_or_default();
            let first = message.lines().next().unwrap_or_default();
            // "Merge branch 'feature' of ../remote" or "Merge remote-tracking branch 'origin/main'".
            let other = first.split('\'').nth(1).map(str::to_owned).unwrap_or_else(|| self.subject_of("MERGE_HEAD"));
            (
                OperationKind::Merge,
                said!("conflicts.merging", "Merging {other} into {branch}", other = other, branch = branch),
                branch.clone(),
                other,
            )
        } else if self.git_path_exists("REVERT_HEAD") {
            let subject = self.subject_of("REVERT_HEAD");
            (
                OperationKind::Revert,
                said!("conflicts.reverting", "Reverting \"{subject}\" on {branch}", subject = subject, branch = branch),
                branch.clone(),
                said!("conflicts.revertOf", "the revert of \"{subject}\"", subject = subject),
            )
        } else if self.git_path_exists("CHERRY_PICK_HEAD") {
            let subject = self.subject_of("CHERRY_PICK_HEAD");
            (
                OperationKind::CherryPick,
                said!(
                    "conflicts.cherryPicking",
                    "Cherry-picking \"{subject}\" onto {branch}",
                    subject = subject,
                    branch = branch
                ),
                branch.clone(),
                format!("\"{subject}\""),
            )
        } else {
            return Ok(None);
        };
        Ok(Some(Operation { kind, title, ours, theirs, conflicted: self.conflicted()? }))
    }

    /// Merges `name` (a branch, a remote branch or a tag) into the current branch.
    pub fn merge(&self, name: &str) -> Result<Outcome, String> {
        self.refuse_checkout(super::refs::Checkout::Merge)?;
        self.refuse_busy()?;
        let name = crate::git::cli::revision(name)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let before = self.head();
        match self.git(&["merge", "--no-edit", "--end-of-options", name], None, &[], None) {
            Ok(_) if self.head() == before => Ok(Outcome::UpToDate),
            Ok(_) => Ok(Outcome::Done),
            Err(_) if self.git_path_exists("MERGE_HEAD") && !self.conflicted()?.is_empty() => Ok(Outcome::Conflicts),
            Err(said) => Err(said),
        }
    }

    /// A conflicted file's three versions and the file as it is now.
    pub fn conflict(&self, path: &str) -> Result<ConflictFile, String> {
        let path = checked(path)?;
        if !self.conflicted()?.iter().any(|known| known == path) {
            return Err(said!("conflicts.noConflicts", "{path} has no conflicts.", path = path));
        }
        let mut binary = false;
        let mut stage = |number: u8| -> Option<String> {
            let bytes = git_bytes(self.root, &["show", &format!(":{number}:./{path}")]).ok()?;
            binary |= bytes.contains(&0);
            Some(String::from_utf8_lossy(&bytes).into_owned())
        };
        let (base, ours, theirs) = (stage(1), stage(2), stage(3));
        let result = std::fs::read(std::path::Path::new(self.root).join(path)).ok().map(|bytes| {
            binary |= bytes.contains(&0);
            String::from_utf8_lossy(&bytes).into_owned()
        });
        Ok(ConflictFile { path: path.to_owned(), base, ours, theirs, result, binary })
    }

    /// Resolves a file by taking one side whole (or its deletion, when that side deleted it), and stages it.
    pub fn take(&self, path: &str, side: Side) -> Result<(), String> {
        let path = checked(path)?;
        self.refuse_filtered(&[path], false)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let number = if side == Side::Ours { 2 } else { 3 };
        let present = git_bytes(self.root, &["show", &format!(":{number}:./{path}")]).is_ok();
        if present {
            let flag = if side == Side::Ours { "--ours" } else { "--theirs" };
            self.git(&["checkout", flag, "--", path], None, &[], None)?;
            self.git(&["add", "--", path], None, &[], None).map(drop)
        } else {
            self.git(&["rm", "-q", "--", path], None, &[], None).map(drop)
        }
    }

    /// Resolves a file with `text` (the result written in the window), or as it is on disk when null, and stages it.
    pub fn resolve(&self, path: &str, text: Option<&str>) -> Result<(), String> {
        let path = checked(path)?;
        self.refuse_filtered(&[path], false)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        if let Some(text) = text {
            std::fs::write(std::path::Path::new(self.root).join(path), text)
                .map_err(|error| format!("{path}: {error}"))?;
        }
        self.git(&["add", "--", path], None, &[], None).map(drop)
    }

    /// Finishes the operation once no file conflicts: the merge commit, or the next step of a revert, cherry-pick or
    /// rebase (which may stop on new conflicts).
    pub fn proceed(&self) -> Result<Outcome, String> {
        let Some(going) = self.operation()? else {
            return Err(said!("conflicts.nothingToContinue", "Nothing is going on to continue."));
        };
        if !going.conflicted.is_empty() {
            let count = going.conflicted.len();
            return Err(said!("conflicts.stillConflicting", "{count} files still conflict.", count = count));
        }
        self.refuse_checkout(super::refs::Checkout::Continue)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let args: &[&str] = match going.kind {
            OperationKind::Merge => &["commit", "--no-edit"],
            OperationKind::Revert => &["revert", "--continue"],
            OperationKind::CherryPick => &["cherry-pick", "--continue"],
            OperationKind::Rebase => &["rebase", "--continue"],
        };
        match self.git(args, None, &[], None) {
            Ok(_) => Ok(if self.operation()?.is_some_and(|now| !now.conflicted.is_empty()) {
                Outcome::Conflicts
            } else {
                Outcome::Done
            }),
            Err(_) if self.operation()?.is_some_and(|now| !now.conflicted.is_empty()) => Ok(Outcome::Conflicts),
            Err(said) => Err(said),
        }
    }

    /// Stops the operation and puts the branch and the files back as they were before it.
    pub fn abort(&self) -> Result<(), String> {
        let Some(going) = self.operation()? else {
            return Err(said!("conflicts.nothingToAbort", "Nothing is going on to abort."));
        };
        self.refuse_checkout(super::refs::Checkout::Abort)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let command = match going.kind {
            OperationKind::Merge => "merge",
            OperationKind::Revert => "revert",
            OperationKind::CherryPick => "cherry-pick",
            OperationKind::Rebase => "rebase",
        };
        self.git(&[command, "--abort"], None, &[], None).map(drop)
    }
}
