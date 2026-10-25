//! The line ranges a change adds and removes, file by file, for the changes by function (`code/changes.rs`): the
//! working tree against a base, or one commit against another, as `git diff -U0` reports them through the safe runner.
//! And the base itself: where the branch left main, the last commit, or any branch, tag or commit.

use serde::Serialize;
use specta::Type;

use super::cli::{git, revision};
use super::history::unquote;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: String,
    /// The path at the base, when the file was renamed or deleted.
    pub old_path: String,
    pub status: FileStatus,
    /// Lines added, on the newer side; lines removed, on the older side.
    pub added: Vec<Range>,
    pub removed: Vec<Range>,
    pub binary: bool,
}

/// The `a b` of `@@ -a,b +c,d @@`: where a hunk starts and how many lines it has (one when the count is left out).
fn range(text: &str) -> Option<Range> {
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    Some(Range { start: start.parse().ok()?, count: count.parse().ok()? })
}

/// Reads `git diff -U0` output: the files and the line ranges each hunk adds and removes.
pub fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            // A first guess from the header (ambiguous when a path holds " b/"); the --- and +++ lines below settle it.
            let (old, new) =
                header.split_once(" b/").or_else(|| header.split_once("\" \"b/")).unwrap_or((header, header));
            let clean = |path: &str| path.trim_matches('"').trim_start_matches("a/").to_owned();
            files.push(FileDiff {
                path: clean(new),
                old_path: clean(old),
                status: FileStatus::Modified,
                added: Vec::new(),
                removed: Vec::new(),
                binary: false,
            });
            continue;
        }
        let Some(current) = files.last_mut() else { continue };
        if let Some(old) = line.strip_prefix("--- ") {
            let old = unquote(old);
            if old != "/dev/null" {
                current.old_path = old.strip_prefix("a/").unwrap_or(&old).to_owned();
            }
        } else if let Some(new) = line.strip_prefix("+++ ") {
            let new = unquote(new);
            current.path = if new == "/dev/null" {
                current.old_path.clone()
            } else {
                new.strip_prefix("b/").unwrap_or(&new).to_owned()
            };
        } else if line.starts_with("new file mode") {
            current.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode") {
            current.status = FileStatus::Deleted;
        } else if let Some(old) = line.strip_prefix("rename from ") {
            current.old_path = unquote(old);
            current.status = FileStatus::Renamed;
        } else if let Some(new) = line.strip_prefix("rename to ") {
            current.path = unquote(new);
        } else if line.starts_with("Binary files ") {
            current.binary = true;
        } else if let Some(hunk) = line.strip_prefix("@@ -") {
            let mut parts = hunk.split(' ');
            let removed = parts.next().and_then(range);
            let added = parts.next().and_then(|part| part.strip_prefix('+')).and_then(range);
            if let (Some(removed), Some(added)) = (removed, added) {
                if removed.count > 0 {
                    current.removed.push(removed);
                }
                if added.count > 0 {
                    current.added.push(added);
                }
            }
        }
    }
    files
}

/// The files the working tree changes compared with `sha` (staged, unstaged and untracked), with their line ranges.
pub fn working_tree_diff(root: &str, sha: &str) -> Result<Vec<FileDiff>, String> {
    let mut files = parse_diff(&git(root, &["diff", "--no-color", "-U0", "-M", "--relative", sha, "--"])?);
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let listed: std::collections::HashSet<String> = files.iter().map(|file| file.path.clone()).collect();
    for path in untracked.split('\0').filter(|path| !path.is_empty()) {
        if listed.contains(path) {
            continue;
        }
        files.push(FileDiff {
            path: path.to_owned(),
            old_path: path.to_owned(),
            status: FileStatus::Added,
            added: vec![Range { start: 1, count: u32::MAX }],
            removed: Vec::new(),
            binary: false,
        });
    }
    Ok(files)
}

/// The files the staged changes change compared with the last commit (everything staged when there is none yet).
pub fn staged_diff(root: &str) -> Result<Vec<FileDiff>, String> {
    Ok(parse_diff(&git(root, &["diff", "--cached", "--no-color", "-U0", "-M", "--relative", "--"])?))
}

/// The staged text of `path` (relative to the folder), or None when it is not in the index or not text.
pub fn index_text(root: &str, path: &str) -> Option<String> {
    git(root, &["show", "--end-of-options", &format!(":./{path}")]).ok()
}

/// The line ranges each file's hunks add and remove between `from` and `to` (`from` None: everything `to` has, its
/// first commit).
pub fn commit_diff(root: &str, from: Option<&str>, to: &str) -> Result<Vec<FileDiff>, String> {
    let flags = ["--no-color", "-M", "--relative", "-p", "-U0", "--end-of-options"];
    let to = revision(to)?;
    let out = match from {
        Some(from) => {
            let from = revision(from)?;
            git(root, &[&["diff"][..], &flags, &[from, to, "--"]].concat())?
        }
        None => git(root, &[&["diff-tree", "-r", "--root", "--no-commit-id"][..], &flags, &[to, "--"]].concat())?,
    };
    Ok(parse_diff(&out))
}

/// Whether `root` is inside a git work tree.
pub fn is_repository(root: &str) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"]).map(|out| out.trim() == "true") == Ok(true)
}

/* ---------- the base ---------- */

/// A branch, tag or commit's commit, or None when it names none here.
fn commit_of(root: &str, name: &str) -> Option<String> {
    if name.is_empty() || name.starts_with('-') {
        return None;
    }
    let spec = format!("{name}^{{commit}}");
    git(root, &["rev-parse", "--verify", "--quiet", "--end-of-options", &spec])
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|sha| !sha.is_empty())
}

/// The branch work is merged into: origin's default branch, else main, master, trunk or develop, local or on origin.
pub(crate) fn main_branch(root: &str) -> Option<String> {
    if let Ok(out) = git(root, &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]) {
        let name = out.trim();
        if let Some(name) = name.strip_prefix("refs/remotes/") {
            return Some(name.to_owned());
        }
    }
    for name in ["main", "master", "trunk", "develop"] {
        if commit_of(root, &format!("refs/heads/{name}")).is_some() {
            return Some(name.to_owned());
        }
        if commit_of(root, &format!("refs/remotes/origin/{name}")).is_some() {
            return Some(format!("origin/{name}"));
        }
    }
    None
}

/// The branch checked out (None when HEAD is detached) and its commit (None before the first commit).
#[derive(Debug, Clone, Serialize, Type)]
pub struct Head {
    pub branch: Option<String>,
    pub sha: Option<String>,
}

pub fn head(root: &str) -> Head {
    let branch = git(root, &["symbolic-ref", "--short", "--quiet", "HEAD"])
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|name| !name.is_empty());
    Head { branch, sha: commit_of(root, "HEAD") }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum BaseKind {
    MergeBase,
    Uncommitted,
    Ref,
}

/// What the working tree is compared with.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Base {
    pub kind: BaseKind,
    /// For people: "where feature/x left main", "the last commit", "v1.2".
    pub label: String,
    /// The branch or ref compared with, when there is one.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub sha: String,
    /// Commits on HEAD since the base, and on the base's branch since HEAD left it (merge-base only).
    pub ahead: u32,
    pub behind: u32,
}

/// What to compare with, as the window asks for it.
pub enum Wanted<'a> {
    /// Where the branch left `branch` (by default the main branch).
    MergeBase(Option<&'a str>),
    Uncommitted,
    Ref(&'a str),
}

pub fn resolve_base(root: &str, wanted: Wanted, head: &Head) -> Result<Base, String> {
    let sha = head.sha.clone().ok_or("This repository has no commit yet.")?;
    let last = |label: &str| Base {
        kind: BaseKind::Uncommitted,
        label: label.into(),
        reference: Some("HEAD".into()),
        sha: sha.clone(),
        ahead: 0,
        behind: 0,
    };
    let target = match wanted {
        Wanted::Uncommitted => return Ok(last("the last commit")),
        Wanted::Ref(name) => {
            let found =
                commit_of(root, name).ok_or_else(|| format!("\"{name}\" is not a commit, branch or tag here."))?;
            return Ok(Base {
                kind: BaseKind::Ref,
                label: name.into(),
                reference: Some(name.into()),
                sha: found,
                ahead: 0,
                behind: 0,
            });
        }
        Wanted::MergeBase(branch) => match branch.map(str::to_owned).or_else(|| main_branch(root)) {
            Some(target) => target,
            None => return Ok(last("the last commit (no main branch found)")),
        },
    };
    let target_sha = commit_of(root, &target).ok_or_else(|| format!("\"{target}\" is not a branch here."))?;
    let base = git(root, &["merge-base", &sha, &target_sha])?.trim().to_owned();
    let counts = git(root, &["rev-list", "--left-right", "--count", &format!("{target_sha}...{sha}")])?;
    let mut counts = counts.split_whitespace().map(|count| count.parse().unwrap_or(0));
    let (behind, ahead) = (counts.next().unwrap_or(0), counts.next().unwrap_or(0));
    let branch = head.branch.as_deref();
    let on_target = branch.is_some_and(|branch| target == branch || target.ends_with(&format!("/{branch}")));
    let label = if on_target {
        format!("{target} (uncommitted work)")
    } else {
        format!("where {} left {target}", branch.unwrap_or("HEAD"))
    };
    Ok(Base { kind: BaseKind::MergeBase, label, reference: Some(target), sha: base, ahead, behind })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_diff_with_a_rename_a_new_file_and_hunks() {
        let text = "diff --git a/old.ts b/new.ts\nsimilarity index 90%\nrename from old.ts\nrename to new.ts\n--- a/old.ts\n+++ b/new.ts\n@@ -2 +2,2 @@\n-a\n+b\n+c\ndiff --git a/fresh.ts b/fresh.ts\nnew file mode 100644\n--- /dev/null\n+++ b/fresh.ts\n@@ -0,0 +1,3 @@\ndiff --git a/gone.ts b/gone.ts\ndeleted file mode 100644\n--- a/gone.ts\n+++ /dev/null\n@@ -1,4 +0,0 @@\n";
        let files = parse_diff(text);
        assert_eq!(files.len(), 3);
        assert_eq!(
            (files[0].old_path.as_str(), files[0].path.as_str(), files[0].status),
            ("old.ts", "new.ts", FileStatus::Renamed)
        );
        assert_eq!(files[0].removed, [Range { start: 2, count: 1 }]);
        assert_eq!(files[0].added, [Range { start: 2, count: 2 }]);
        assert_eq!((files[1].status, files[1].added.clone()), (FileStatus::Added, vec![Range { start: 1, count: 3 }]));
        assert!(files[1].removed.is_empty());
        assert_eq!((files[2].path.as_str(), files[2].status), ("gone.ts", FileStatus::Deleted));
        assert_eq!(files[2].removed, [Range { start: 1, count: 4 }]);
    }
}
