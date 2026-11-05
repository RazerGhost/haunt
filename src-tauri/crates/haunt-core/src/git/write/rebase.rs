//! Interactive rebase: under the rules of `write/mod.rs`. The commits of the current
//! branch after a base are listed (`rebase_plan`); the window reorders them and picks what happens to each: keep,
//! reword, squash into the one before (both messages kept), fixup (the one before's message only) or drop. `rebase`
//! then runs `git rebase -i` with that list written for it, so no editor opens:
//!
//! - The to-do list is written to a file and copied over git's own (`GIT_SEQUENCE_EDITOR`); a squash's combined
//!   message is kept as git writes it (`GIT_EDITOR=:`).
//! - A reworded commit is picked, then amended with its new message from a file (`exec git commit --amend -F …`).
//! - A rebase that stops on conflicts leaves them for the conflict view (`conflicts.rs`), which continues or aborts it.
//! - Merge commits in the range are refused (rebasing them needs `--rebase-merges`, which this does not do), and so is
//!   an untrusted folder whose files go through filters, since rebasing writes files.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{queue, Outcome, Writer};
use crate::git::cli::git as read;
use crate::git::types::{CommitInfo, COMMIT_FORMAT};

/// What happens to one commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RebaseAction {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
}

/// One commit of the new order, and what happens to it.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct RebaseStep {
    pub sha: String,
    pub action: RebaseAction,
    /// The new message, for a reword.
    pub message: Option<String>,
}

impl Writer<'_> {
    /// The commits of the current branch after `onto`, oldest first: what an interactive rebase onto it works on.
    pub fn rebase_plan(&self, onto: &str) -> Result<Vec<CommitInfo>, String> {
        let onto = self.commit_of(onto)?;
        self.current_branch()?;
        let range = format!("{onto}..HEAD");
        if !read(self.root, &["rev-list", "--merges", "-n", "1", &range])?.trim().is_empty() {
            return Err(said!(
                "rebase.merges",
                "There is a merge commit after that one. Rebasing across merges is not something Haunt does; pick a later commit."
            ));
        }
        let out = read(self.root, &["log", "--reverse", &format!("--format={COMMIT_FORMAT}"), &range])?;
        Ok(CommitInfo::all(&out))
    }

    /// Rebases the current branch onto `onto` with `steps`: every commit of the plan once, in the new order.
    pub fn rebase(&self, onto: &str, steps: &[RebaseStep]) -> Result<Outcome, String> {
        self.refuse_checkout(super::refs::Checkout::Rebase)?;
        self.refuse_busy()?;
        let plan = self.rebase_plan(onto)?;
        let onto = self.commit_of(onto)?;
        let mut wanted: Vec<&str> = steps.iter().map(|step| step.sha.as_str()).collect();
        let mut known: Vec<&str> = plan.iter().map(|commit| commit.sha.as_str()).collect();
        wanted.sort_unstable();
        known.sort_unstable();
        if wanted != known {
            return Err(said!(
                "rebase.changed",
                "The list of commits changed since it was read. Open the rebase again."
            ));
        }
        match steps.iter().find(|step| step.action != RebaseAction::Drop) {
            Some(first) if matches!(first.action, RebaseAction::Squash | RebaseAction::Fixup) => {
                return Err(said!(
                    "rebase.squashFirst",
                    "The first commit kept cannot be squashed: there is nothing before it to squash into."
                ));
            }
            None => return Err(said!("rebase.allDropped", "Every commit would be dropped. Reset the branch instead.")),
            _ => {}
        }
        if steps.iter().any(|step| {
            step.action == RebaseAction::Reword
                && step.message.as_deref().is_none_or(|message| message.trim().is_empty())
        }) {
            return Err(said!("rebase.noMessage", "A reworded commit needs its new message."));
        }

        let dir = std::env::temp_dir().join(format!("haunt-rebase-{}-{}", std::process::id(), super::now_seconds()));
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path_of = |name: &str| dir.join(name).to_string_lossy().replace('\\', "/");
        let mut todo = String::new();
        for (at, step) in steps.iter().enumerate() {
            let word = match step.action {
                RebaseAction::Pick | RebaseAction::Reword => "pick",
                RebaseAction::Squash => "squash",
                RebaseAction::Fixup => "fixup",
                RebaseAction::Drop => "drop",
            };
            todo.push_str(&format!("{word} {}\n", step.sha));
            if step.action == RebaseAction::Reword {
                let file = path_of(&format!("message-{at}"));
                std::fs::write(&file, step.message.as_deref().unwrap_or_default().trim())
                    .map_err(|error| error.to_string())?;
                todo.push_str(&format!(
                    "exec git commit --amend --allow-empty --cleanup=whitespace -q -F \"{file}\"\n"
                ));
            }
        }
        let todo_file = path_of("todo");
        std::fs::write(&todo_file, todo).map_err(|error| error.to_string())?;

        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let editor = format!("cp \"{todo_file}\"");
        let result = self.git(
            &["rebase", "-i", "--no-autosquash", &onto],
            None,
            &[("GIT_SEQUENCE_EDITOR", editor.as_str())],
            None,
        );
        let outcome = match result {
            Ok(_) => Ok(Outcome::Done),
            Err(said) => match self.operation()? {
                Some(going) if !going.conflicted.is_empty() => Ok(Outcome::Conflicts),
                _ => Err(said),
            },
        };
        // The message files are read by the exec lines as the rebase goes; they may still be needed after a conflict.
        if outcome != Ok(Outcome::Conflicts) {
            let _ = std::fs::remove_dir_all(&dir);
        }
        outcome
    }
}
