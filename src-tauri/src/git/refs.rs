//! The sidebar's refs, through git: branches with their upstreams (`git for-each-ref`, which knows ahead and behind),
//! remote branches, tags and stashes. How many files have changes not yet committed is its own call
//! (`local_changes`): `git status` is the slow part, and only edits and staging change it.

use super::cli::git;
use super::types::{BranchRef, Refs, Remote, RemoteBranchRef, StashRef, TagRef};

const FIELD: char = '\x1f';

/// The local branches whose commits are all on the current branch (`git branch --merged HEAD`), but the current one:
/// deleting them loses nothing.
pub fn merged_branches(root: &str) -> Result<Vec<String>, String> {
    let current = git(root, &["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default().trim().to_owned();
    let out = git(root, &["branch", "--format=%(refname:short)", "--merged", "HEAD"])?;
    Ok(out.lines().map(str::trim).filter(|name| !name.is_empty() && *name != current).map(str::to_owned).collect())
}

/// "ahead 2, behind 1", "ahead 3", "gone" or "" (from %(upstream:track,nobracket)).
fn track(text: &str) -> (u32, u32, bool) {
    let count = |word: &str| {
        text.split(", ")
            .find_map(|part| part.strip_prefix(word))
            .and_then(|number| number.trim().parse().ok())
            .unwrap_or(0)
    };
    (count("ahead "), count("behind "), text.contains("gone"))
}

/// The branch checked out (null when HEAD is detached) and HEAD's commit (null in a repository without one).
pub fn head(root: &str) -> (Option<String>, Option<String>) {
    let branch = git(root, &["symbolic-ref", "--short", "--quiet", "HEAD"])
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|name| !name.is_empty());
    let sha = git(root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|sha| !sha.is_empty());
    (branch, sha)
}

fn in_work_tree(root: &str) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"]).map(|out| out.trim() == "true") == Ok(true)
}

/// The refs, or None when `root` is not in a git repository. Read after every change to git, so in as few calls as
/// can be: whether it is a work tree and HEAD's commit in one, the refs (with the current branch marked) in another,
/// and the stashes.
pub fn refs(root: &str) -> Result<Option<Refs>, String> {
    let sha = match git(root, &["rev-parse", "--is-inside-work-tree", "HEAD^{commit}"]) {
        Ok(out) => {
            let mut lines = out.lines().map(str::trim);
            if lines.next() != Some("true") {
                return Ok(None);
            }
            lines.next().filter(|sha| !sha.is_empty()).map(str::to_owned)
        }
        // A repository with no commit yet has no HEAD to name; outside one, there is nothing to list.
        Err(_) if in_work_tree(root) => None,
        Err(_) => return Ok(None),
    };
    let format = [
        "%(refname)",
        "%(objectname)",
        "%(*objectname)",
        "%(upstream:short)",
        "%(upstream:track,nobracket)",
        "%(creatordate:unix)",
        "%(contents:subject)",
        "%(HEAD)",
    ]
    .join("%1f");
    let listed =
        git(root, &["for-each-ref", &format!("--format={format}"), "refs/heads", "refs/remotes", "refs/tags"])?;
    let stashes = git(root, &["stash", "list", "--format=%gd%x1f%ct%x1f%H%x1f%gs"]).unwrap_or_default();
    let mut out =
        Refs { branch: None, sha, branches: Vec::new(), remotes: Vec::new(), tags: Vec::new(), stashes: Vec::new() };
    for line in listed.lines().filter(|line| !line.is_empty()) {
        let parts: Vec<&str> = line.split(FIELD).collect();
        let field = |at: usize| parts.get(at).copied().unwrap_or_default();
        let (reference, sha, peeled, upstream, tracking, time, subject, is_head) =
            (field(0), field(1), field(2), field(3), field(4), field(5), field(6), field(7));
        let time: i64 = time.parse().unwrap_or(0);
        if let Some(name) = reference.strip_prefix("refs/heads/") {
            let (ahead, behind, gone) = track(tracking);
            out.branches.push(BranchRef {
                name: name.into(),
                sha: sha.into(),
                current: is_head == "*",
                upstream: (!upstream.is_empty()).then(|| upstream.into()),
                ahead,
                behind,
                gone,
                subject: subject.into(),
                time,
            });
        } else if let Some(rest) = reference.strip_prefix("refs/remotes/") {
            // refs/remotes/origin/HEAD only points at the remote's default branch.
            let Some((remote, name)) = rest.split_once('/').filter(|_| !rest.ends_with("/HEAD")) else { continue };
            let branch = RemoteBranchRef { name: name.into(), sha: sha.into(), subject: subject.into(), time };
            match out.remotes.iter_mut().find(|known| known.name == remote) {
                Some(known) => known.branches.push(branch),
                None => out.remotes.push(Remote { name: remote.into(), branches: vec![branch] }),
            }
        } else if let Some(name) = reference.strip_prefix("refs/tags/") {
            // An annotated tag points at a tag object; %(*objectname) is the commit it tags.
            out.tags.push(TagRef {
                name: name.into(),
                sha: if peeled.is_empty() { sha.into() } else { peeled.into() },
                time,
            });
        }
    }
    // The branch HEAD is on, as the list marks it (none when HEAD is detached).
    out.branch = out.branches.iter().find(|branch| branch.current).map(|branch| branch.name.clone());
    if out.branch.is_none() {
        // A new repository's branch has no commit, so the list does not have it yet.
        out.branch = head(root).0.filter(|_| out.sha.is_none());
    }
    out.remotes.sort_by(|a, b| a.name.cmp(&b.name));
    out.tags.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.name.cmp(&b.name)));
    out.stashes = stashes
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split(FIELD);
            let reference = parts.next().unwrap_or_default();
            let time = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let sha = parts.next().unwrap_or_default().to_owned();
            let index =
                reference.split_once('{').and_then(|(_, rest)| rest.trim_end_matches('}').parse().ok()).unwrap_or(0);
            StashRef { index, sha, message: parts.next().unwrap_or_default().into(), time }
        })
        .collect();
    Ok(Some(out))
}

/// Files with changes not yet committed (staged, unstaged or untracked), as `git status` counts them; None when
/// `root` is not in a git repository.
pub fn local_changes(root: &str) -> Result<Option<u32>, String> {
    if !in_work_tree(root) {
        return Ok(None);
    }
    let status =
        git(root, &["status", "--porcelain=v1", "-z", "--untracked-files=normal", "--ignore-submodules=dirty"])?;
    // -z: "XY path\0", and a rename or copy adds its old path as a second entry.
    let entries: Vec<&str> = status.split('\0').collect();
    let mut count = 0;
    let mut at = 0;
    while at < entries.len() {
        let entry = entries[at];
        at += 1;
        if entry.len() < 4 {
            continue;
        }
        count += 1;
        if entry.starts_with('R') || entry.starts_with('C') {
            at += 1;
        }
    }
    Ok(Some(count))
}
