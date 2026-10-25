//! The commits between two tags with the files each touched, for the release notes (`app/pages/release.vue`), which
//! group them by part.

use serde::Serialize;
use specta::Type;

use super::cli::{git, revision};
use super::types::{CommitInfo, COMMIT_FORMAT};

/// The most commits a release lists.
const MOST: usize = 2000;

#[derive(Debug, Clone, Serialize, Type)]
pub struct ReleaseCommit {
    pub commit: CommitInfo,
    /// The files it touched, relative to the folder.
    pub paths: Vec<String>,
}

/// The commits `to` has that `from` has not (all of `to`'s when `from` is None), newest first, merges left out.
pub fn release_commits(root: &str, from: Option<&str>, to: &str) -> Result<Vec<ReleaseCommit>, String> {
    let to = revision(to)?;
    let range = match from {
        Some(from) => format!("{}..{to}", revision(from)?),
        None => to.to_owned(),
    };
    let out = git(
        root,
        &[
            "log",
            "--no-merges",
            &format!("--max-count={MOST}"),
            &format!("--format=%x1e{COMMIT_FORMAT}"),
            "--name-only",
            "--relative",
            "--end-of-options",
            &range,
            "--",
        ],
    )?;
    Ok(out
        .split('\x1e')
        .filter_map(|record| {
            let mut lines = record.lines();
            let commit = CommitInfo::parse(lines.next()?)?;
            let paths = lines.map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect();
            Some(ReleaseCommit { commit, paths })
        })
        .collect())
}
