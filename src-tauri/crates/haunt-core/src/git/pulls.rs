//! The pull requests fetched as refs (`write/network.rs`, `refs/haunt/pull/<remote>/<n>`): each one's head commit, and
//! where it went apart from the branch work is merged into, so its commits and changes can be read like a branch's
//! without switching to it. Titles and comments live in the host's API, which Haunt does not call: a pull request is
//! named by its number and its newest commit.

use serde::Serialize;
use specta::Type;

use super::cli::git;
use super::types::{CommitInfo, COMMIT_FORMAT};

const FIELD: char = '\x1f';

/// A pull request fetched as a ref.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u32,
    pub remote: String,
    /// Its ref here, `refs/haunt/pull/<remote>/<n>`.
    pub reference: String,
    /// Its head commit, and that commit's subject, author and time.
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub time: i64,
    /// The branch it is compared with (the remote's default branch, else main or master), and where the two went
    /// apart; null when there is no such branch or they share no history.
    pub base_label: Option<String>,
    pub base: Option<String>,
    /// Its commits after `base`.
    pub commits: u32,
}

/// The pull requests fetched so far, newest number first.
pub fn pull_requests(root: &str) -> Result<Vec<PullRequest>, String> {
    let out = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)%1f%(objectname)%1f%(authorname)%1f%(authordate:unix)%1f%(contents:subject)",
            "refs/haunt/pull/",
        ],
    )?;
    let main = super::diff::main_branch(root);
    let mut found: Vec<PullRequest> = out
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(FIELD);
            let mut next = || parts.next().unwrap_or_default().to_owned();
            let (reference, sha, author, time, subject) = (next(), next(), next(), next(), next());
            let rest = reference.strip_prefix("refs/haunt/pull/")?;
            let (remote, number) = rest.rsplit_once('/')?;
            let number = number.parse().ok()?;
            let base = main.as_ref().and_then(|main| {
                git(root, &["merge-base", "--end-of-options", main, &sha]).ok().map(|out| out.trim().to_owned())
            });
            let base = base.filter(|base| !base.is_empty());
            let commits = base
                .as_ref()
                .and_then(|base| git(root, &["rev-list", "--count", &format!("{base}..{sha}")]).ok())
                .and_then(|count| count.trim().parse().ok())
                .unwrap_or(0);
            Some(PullRequest {
                number,
                remote: remote.to_owned(),
                reference,
                sha,
                subject,
                author,
                time: time.parse().unwrap_or(0),
                base_label: base.as_ref().and(main.clone()),
                base,
                commits,
            })
        })
        .collect();
    found.sort_by(|a, b| b.number.cmp(&a.number).then_with(|| a.remote.cmp(&b.remote)));
    Ok(found)
}

/// A pull request's commits after `base`, newest first (at most 200); all of `sha`'s when there is no base.
pub fn pull_commits(root: &str, base: Option<&str>, sha: &str) -> Result<Vec<CommitInfo>, String> {
    let sha = super::cli::revision(sha)?;
    let range = match base {
        Some(base) => format!("{}..{sha}", super::cli::revision(base)?),
        None => sha.to_owned(),
    };
    let out = git(root, &["log", "--max-count=200", &format!("--format={COMMIT_FORMAT}"), &range])?;
    Ok(CommitInfo::all(&out))
}
