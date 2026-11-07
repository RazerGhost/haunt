//! Worktrees: more branches checked out side by side, each in a folder of its own (`git worktree`), opened in tabs of
//! their own. Under the rules of `write/mod.rs`: one write at a time, every command in the activity list; adding one
//! checks files out, so in an untrusted folder whose files go through filters it is refused as switching branches is,
//! and the repository's hooks stay off there as they always do.

use serde::Serialize;
use specta::Type;

use super::{queue, Writer};
use crate::git::cli::{git as read, revision};

/// A worktree of the repository.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    /// Its branch; null when its HEAD is detached.
    pub branch: Option<String>,
    pub head: String,
    /// The repository's own folder, which cannot be removed.
    pub main: bool,
    /// Locked against pruning and removal (`git worktree lock`), with the reason given.
    pub locked: Option<String>,
    /// Its folder is gone: `prune` forgets it, or `repair_worktrees` finds it where it went.
    pub prunable: bool,
    /// Its folder is there, but its `.git` file no longer leads to this repository (the repository's folder moved):
    /// git cannot work in it until `repair_worktrees` writes the way back.
    pub broken: bool,
}

/// The worktrees of the repository at `root`, its own folder first.
pub fn worktrees(root: &str) -> Result<Vec<Worktree>, String> {
    let out = read(root, &["worktree", "list", "--porcelain"])?;
    let mut found = Vec::new();
    for block in out.split("\n\n").filter(|block| !block.trim().is_empty()) {
        let mut worktree = Worktree {
            path: String::new(),
            branch: None,
            head: String::new(),
            main: found.is_empty(),
            locked: None,
            prunable: false,
            broken: false,
        };
        for line in block.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                worktree.path =
                    crate::paths::canonical(path).unwrap_or_else(|| path.replace('/', std::path::MAIN_SEPARATOR_STR));
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                worktree.head = head.to_owned();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                worktree.branch = Some(branch.strip_prefix("refs/heads/").unwrap_or(branch).to_owned());
            } else if line == "locked" || line.starts_with("locked ") {
                worktree.locked = Some(line.strip_prefix("locked").unwrap_or_default().trim().to_owned());
            } else if line == "prunable" || line.starts_with("prunable ") {
                worktree.prunable = true;
            }
        }
        if !worktree.path.is_empty() {
            found.push(worktree);
        }
    }
    let common = read(root, &["rev-parse", "--git-common-dir"])
        .ok()
        .and_then(|common| std::fs::canonicalize(std::path::Path::new(root).join(common.trim())).ok());
    if let Some(common) = common {
        for worktree in found.iter_mut().filter(|worktree| !worktree.main && !worktree.prunable) {
            worktree.broken = !leads_back(std::path::Path::new(&worktree.path), &common);
        }
    }
    Ok(found)
}

/// Whether the worktree in `folder` leads back to the repository whose common git folder is `common`: its `.git` file
/// names a git folder that is there, and whose `commondir` is `common`.
fn leads_back(folder: &std::path::Path, common: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(folder.join(".git")) else {
        // A folder `.git` (made a repository of its own) or none: not a link that can be repaired.
        return true;
    };
    let Some(gitdir) = text.strip_prefix("gitdir:").map(str::trim) else { return true };
    let gitdir = folder.join(gitdir);
    let Ok(back) = std::fs::read_to_string(gitdir.join("commondir")) else { return false };
    std::fs::canonicalize(gitdir.join(back.trim())).is_ok_and(|found| found == common)
}

impl Writer<'_> {
    /// Checks a branch out in a new worktree at `path` (absolute, a folder that does not exist yet or is empty): an
    /// existing branch, or a new one made at `at` (HEAD by default). Returns the worktree's folder.
    pub fn add_worktree(&self, path: &str, branch: &str, new: bool, at: Option<&str>) -> Result<String, String> {
        let folder = std::path::Path::new(path);
        if !folder.is_absolute() {
            return Err(said!("worktrees.notFull", "A worktree's folder is a full path."));
        }
        if folder.exists() && std::fs::read_dir(folder).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
            return Err(said!(
                "worktrees.notEmpty",
                "{path} is there already and not empty: pick another folder.",
                path = path
            ));
        }
        let branch = self.branch_name(branch)?;
        self.refuse_checkout(super::refs::Checkout::Worktree)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        if new {
            let at = at.map(revision).transpose()?.unwrap_or("HEAD");
            self.git(&["worktree", "add", "-b", branch, "--end-of-options", path, at], None, &[], None)?;
        } else {
            if read(self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_err() {
                return Err(said!("worktrees.noBranch", "There is no branch called \"{branch}\".", branch = branch));
            }
            self.git(&["worktree", "add", "--end-of-options", path, branch], None, &[], None)?;
        }
        Ok(crate::paths::canonical(path).unwrap_or_else(|| path.to_owned()))
    }

    /// Removes a worktree (its folder; the branch stays). One with changes not committed is refused unless `force`.
    pub fn remove_worktree(&self, path: &str, force: bool) -> Result<(), String> {
        let wanted = crate::paths::canonical(path).unwrap_or_else(|| path.to_owned());
        let known = worktrees(self.root)?;
        let worktree = known
            .iter()
            .find(|worktree| worktree.path == wanted)
            .ok_or_else(|| said!("worktrees.unknown", "That is not a worktree of this repository."))?;
        if worktree.main {
            return Err(said!("worktrees.main", "The repository's own folder is not a worktree to remove."));
        }
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.extend(["--end-of-options", worktree.path.as_str()]);
        self.git(&args, None, &[], None).map(drop).map_err(|error| {
            if error.contains("modified or untracked files") {
                said!("worktrees.changes", "{path} has changes not committed yet.", path = worktree.path)
            } else {
                error
            }
        })
    }

    /// Writes the links between the repository and its worktrees again (`git worktree repair`): after the
    /// repository's folder moved (`paths`: the worktrees that no longer lead back), or a worktree's folder moved
    /// (`paths`: where it is now).
    pub fn repair_worktrees(&self, paths: &[String]) -> Result<(), String> {
        if paths.is_empty() || paths.iter().any(|path| !std::path::Path::new(path).is_absolute()) {
            return Err(said!("worktrees.notFull", "A worktree's folder is a full path."));
        }
        let queue = queue(self.root);
        let _turn = queue.lock();
        let mut args = vec!["worktree", "repair", "--end-of-options"];
        args.extend(paths.iter().map(String::as_str));
        self.git(&args, None, &[], None).map(drop)
    }

    /// Forgets the worktrees whose folders are gone.
    pub fn prune_worktrees(&self) -> Result<(), String> {
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.git(&["worktree", "prune"], None, &[], None).map(drop)
    }
}
