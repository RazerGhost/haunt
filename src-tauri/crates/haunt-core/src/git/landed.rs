//! Commits on other branches whose changes the main line already has under another sha: cherry-picked or rebased in
//! (the same patch, as `git cherry` finds), or squash-merged (the branch's whole change is one commit there). git
//! still counts these branches as not merged; the commit list dims their commits and Branch cleanup offers them.
//!
//! Patches are compared by `git patch-id --stable`, which ignores line numbers and whitespace: the main line's newest
//! commits once, then each branch's own commits and its change as a whole.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

use serde::Serialize;
use specta::Type;

use super::cli::{git, pipe};

/// How many of the main line's newest commits are compared with.
const BASE_COMMITS: &str = "--max-count=3000";
/// How many of a branch's own commits are compared.
const BRANCH_COMMITS: &str = "--max-count=500";
/// How many branches are looked at, most recently changed first.
const BRANCHES: &str = "--count=40";

#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Landed {
    /// The main line compared with (`origin/main`, else `main` or `master`); null when there is none.
    pub base: Option<String>,
    /// Branch commits whose changes the main line has, each with the commit there that has them.
    pub commits: Vec<LandedCommit>,
    /// The branches and remote branches with commits the main line has (git counts none of them as merged).
    pub branches: Vec<LandedBranch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LandedBranch {
    /// Its full name: `refs/heads/feature` or `refs/remotes/origin/feature`.
    pub reference: String,
    /// Its commits since it left the main line (at most 500 counted).
    pub own: u32,
    /// How many of them the main line has: all of them when it was squash-merged.
    pub landed: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LandedCommit {
    pub sha: String,
    pub landed_as: String,
}

/// The main line: where origin/HEAD points, else origin/main, origin/master, main or master.
fn base(root: &str) -> Option<String> {
    if let Ok(out) = git(root, &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]) {
        let name = out.trim();
        if !name.is_empty() {
            return Some(name.to_owned());
        }
    }
    ["refs/remotes/origin/main", "refs/remotes/origin/master", "refs/heads/main", "refs/heads/master"]
        .into_iter()
        .find(|name| git(root, &["rev-parse", "--verify", "--quiet", name]).is_ok())
        .map(|name| name.trim_start_matches("refs/remotes/").trim_start_matches("refs/heads/").to_owned())
}

/// `git <args>` fed to `git patch-id --stable`: (patch id, commit) pairs.
fn patch_ids(root: &str, args: &[&str]) -> Vec<(String, String)> {
    let Ok(mut lines) = pipe(root, args, &["patch-id", "--stable"]) else { return Vec::new() };
    let mut out = Vec::new();
    while let Some(Ok(line)) = lines.next_line() {
        if let Some((id, sha)) = line.split_once(' ') {
            out.push((id.to_owned(), sha.trim().to_owned()));
        }
    }
    out
}

/// What one branch tip has on the main line: its commits there under other shas, and how many of its own it has.
#[derive(Clone)]
struct Tip {
    commits: Vec<(String, String)>,
    own: usize,
    landed: usize,
}

/// Per repository: the main line's patches (by its tip) and each branch tip's answer, so reading again after a commit
/// or fetch compares only what moved.
struct Kept {
    base_tip: String,
    known: Arc<HashMap<String, String>>,
    tips: HashMap<String, Tip>,
}

fn kept() -> &'static Mutex<HashMap<String, Kept>> {
    static KEPT: OnceLock<Mutex<HashMap<String, Kept>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The main line's newest commits by patch id.
fn base_patches(root: &str, base: &str) -> HashMap<String, String> {
    let mut known = HashMap::new();
    for (id, sha) in
        patch_ids(root, &["log", "-p", "--no-merges", "--no-renames", "--pretty=medium", BASE_COMMITS, base, "--"])
    {
        known.entry(id).or_insert(sha);
    }
    known
}

/// One branch compared with the main line; None when it has no commits of its own.
fn tip(root: &str, base: &str, reference: &str, known: &HashMap<String, String>) -> Option<Tip> {
    let start = git(root, &["merge-base", base, reference]).ok()?;
    let start = start.trim();
    let range = format!("{start}..{reference}");
    let own: Vec<String> = git(root, &["rev-list", BRANCH_COMMITS, &range])
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    if own.is_empty() {
        return None;
    }
    let mut commits = Vec::new();
    let mut patches = 0;
    for (id, sha) in
        patch_ids(root, &["log", "-p", "--no-merges", "--no-renames", "--pretty=medium", BRANCH_COMMITS, &range, "--"])
    {
        patches += 1;
        if let Some(there) = known.get(&id) {
            commits.push((sha, there.clone()));
        }
    }
    let mut landed = commits.len();
    if landed < patches {
        // The branch's change as a whole, as a squash merge makes it one commit.
        let whole = patch_ids(root, &["diff", "--no-renames", start, reference, "--"])
            .into_iter()
            .find_map(|(id, _)| known.get(&id).cloned());
        if let Some(there) = whole {
            commits = own.iter().map(|sha| (sha.clone(), there.clone())).collect();
            landed = own.len();
        }
    }
    // A branch with merges of its own lands in full only when its change as a whole has: they have no patch to match.
    Some(Tip { commits, own: own.len(), landed })
}

/// What the main line already has of the other branches.
pub fn landed(root: &str) -> Result<Landed, String> {
    let Some(base) = base(root) else { return Ok(Landed::default()) };
    let base_tip = git(root, &["rev-parse", "--verify", "--quiet", &base])?.trim().to_owned();
    let listed = git(
        root,
        &[
            "for-each-ref",
            &format!("--no-merged={base}"),
            "--sort=-committerdate",
            BRANCHES,
            "--format=%(objectname) %(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let (known, mut tips) = {
        let mut all = kept().lock();
        match all.remove(root) {
            Some(was) if was.base_tip == base_tip => (was.known, was.tips),
            _ => (Arc::new(base_patches(root, &base)), HashMap::new()),
        }
    };
    let mut out = Landed { base: Some(base.clone()), ..Landed::default() };
    let mut found: HashMap<String, String> = HashMap::new();
    let mut seen: HashMap<String, Tip> = HashMap::new();
    if !known.is_empty() {
        for line in listed.lines().map(str::trim) {
            let Some((sha, reference)) = line.split_once(' ') else { continue };
            if reference.ends_with("/HEAD") {
                continue;
            }
            let answer = match tips.remove(sha).or_else(|| seen.get(sha).cloned()) {
                Some(answer) => Some(answer),
                None => tip(root, &base, reference, &known),
            };
            let Some(answer) = answer else { continue };
            for (commit, there) in &answer.commits {
                found.entry(commit.clone()).or_insert_with(|| there.clone());
            }
            if answer.landed > 0 {
                let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
                out.branches.push(LandedBranch {
                    reference: reference.to_owned(),
                    own: count(answer.own),
                    landed: count(answer.landed),
                });
            }
            seen.insert(sha.to_owned(), answer);
        }
    }
    kept().lock().insert(root.to_owned(), Kept { base_tip, known, tips: seen });
    out.commits = found.into_iter().map(|(sha, landed_as)| LandedCommit { sha, landed_as }).collect();
    out.commits.sort_by(|a, b| a.sha.cmp(&b.sha));
    out.branches.sort_by(|a, b| a.reference.cmp(&b.reference));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Repo;

    #[test]
    fn finds_commits_cherry_picked_and_branches_squash_merged() {
        let r = Repo::new();
        r.write("a.txt", "one\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-qm", "first"]);
        let main = r.sh(&["symbolic-ref", "--short", "HEAD"]).trim().to_owned();

        // A branch squash-merged into the main line.
        r.sh(&["switch", "-qc", "squashed"]);
        r.write("b.txt", "b\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-qm", "b"]);
        r.write("b.txt", "b\nbb\n");
        r.sh(&["commit", "-qam", "bb"]);
        let squashed: Vec<String> =
            r.sh(&["rev-list", &format!("{main}..squashed")]).lines().map(str::to_owned).collect();

        // A branch with one commit picked into the main line and one not.
        r.sh(&["switch", "-q", &main]);
        r.sh(&["switch", "-qc", "picked"]);
        r.write("c.txt", "c\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-qm", "c"]);
        let c = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        r.write("d.txt", "d\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-qm", "d"]);

        r.sh(&["switch", "-q", &main]);
        r.sh(&["merge", "-q", "--squash", "squashed"]);
        r.sh(&["commit", "-qm", "squashed in"]);
        let squash = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        r.sh(&["cherry-pick", &c]);
        let pick = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();

        let found = landed(&r.root).unwrap();
        assert_eq!(found.base.as_deref(), Some(main.as_str()));
        let branches: Vec<(&str, u32, u32)> =
            found.branches.iter().map(|branch| (branch.reference.as_str(), branch.own, branch.landed)).collect();
        assert_eq!(branches, [("refs/heads/picked", 2, 1), ("refs/heads/squashed", 2, 2)]);
        let at =
            |sha: &str| found.commits.iter().find(|commit| commit.sha == sha).map(|commit| commit.landed_as.clone());
        for sha in &squashed {
            assert_eq!(at(sha).as_deref(), Some(squash.as_str()));
        }
        assert_eq!(at(&c).as_deref(), Some(pick.as_str()));
        assert_eq!(found.commits.len(), squashed.len() + 1);
        // Read again with nothing moved: the same, from what was kept.
        let again = landed(&r.root).unwrap();
        assert_eq!(again.branches, found.branches);
        assert_eq!(again.commits.len(), found.commits.len());

        // The commit list leaves out a branch the main line has in full, when asked.
        let listed = |hide: Option<Vec<String>>| {
            let query = crate::git::types::LogQuery { hide, ..Default::default() };
            crate::git::log(&r.root, &query).unwrap().commits.into_iter().map(|commit| commit.sha).collect::<Vec<_>>()
        };
        assert!(squashed.iter().all(|sha| listed(None).contains(sha)));
        let hidden = listed(Some(vec!["refs/heads/squashed".into()]));
        assert!(squashed.iter().all(|sha| !hidden.contains(sha)));
        assert!(hidden.contains(&c));
    }
}
