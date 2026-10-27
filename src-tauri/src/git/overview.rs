//! A repository at a glance, for the repository manager: read by its path, without opening a tab, through the
//! read-only `git` of `cli.rs`, so nothing of the repository's own runs (the same as the recent list).

use std::path::Path;

use serde::Serialize;
use specta::Type;

use super::cli::git;
use super::local::{status, LocalStatus};
use super::remote::{web_page, WebRemote};
use super::types::{BranchRef, CommitInfo, TagRef, COMMIT_FORMAT};

/// How many of the latest commits, and of the tags, the overview lists.
const LATEST: u32 = 20;
const TAGS: usize = 30;
/// A README longer than this is cut.
const README_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub path: String,
    pub name: String,
    /// None when HEAD is detached.
    pub branch: Option<String>,
    pub status: LocalStatus,
    /// Commits reachable from HEAD.
    pub commits: u32,
    pub first: Option<CommitInfo>,
    pub last: Option<CommitInfo>,
    pub remotes: Vec<RemoteInfo>,
    pub branches: Vec<BranchRef>,
    /// The newest tags first, at most `TAGS`; `tag_count` has them all.
    pub tags: Vec<TagRef>,
    pub tag_count: u32,
    /// The latest commits on HEAD, newest first.
    pub latest: Vec<CommitInfo>,
    pub readme: Option<Readme>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
    /// Its project page, when the URL is a host's.
    pub web: Option<WebRemote>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct Readme {
    /// Its file name, as the folder has it.
    pub name: String,
    pub text: String,
    /// Longer than Haunt shows: the start only.
    pub cut: bool,
}

/// Who committed how much, when, and what the repository holds.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Statistics {
    /// The authors with the most commits on HEAD, most first.
    pub authors: Vec<AuthorCount>,
    /// Commits on HEAD per month over the last year, oldest first; a month without any is there with 0.
    pub months: Vec<MonthCount>,
    /// Files git tracks.
    pub files: u32,
    /// Tracked files by extension, most first ("" for none).
    pub extensions: Vec<ExtensionCount>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AuthorCount {
    pub name: String,
    pub email: String,
    pub commits: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct MonthCount {
    /// "2026-10".
    pub month: String,
    pub commits: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ExtensionCount {
    pub extension: String,
    pub files: u32,
}

/// The folder as a repository to read: its one form, and a git work tree at its top.
pub fn repository(path: &str) -> Result<String, String> {
    let root = crate::paths::canonical(path).ok_or_else(|| said!("overview.noFolder", "That folder is not there."))?;
    let top = git(&root, &["rev-parse", "--show-toplevel"])
        .map_err(|_| said!("overview.notRepository", "That folder is not a git repository."))?;
    Ok(crate::paths::canonical(top.trim()).unwrap_or(root))
}

pub fn overview(path: &str) -> Result<Overview, String> {
    let root = repository(path)?;
    let refs =
        super::refs(&root)?.ok_or_else(|| said!("overview.notRepository", "That folder is not a git repository."))?;
    let has_head = refs.sha.is_some();
    let commits = if has_head { git(&root, &["rev-list", "--count", "HEAD"])?.trim().parse().unwrap_or(0) } else { 0 };
    let latest = if has_head {
        CommitInfo::all(&git(
            &root,
            &["log", &format!("--max-count={LATEST}"), &format!("--format={COMMIT_FORMAT}"), "HEAD"],
        )?)
    } else {
        Vec::new()
    };
    // The oldest root commit: a history that joined another one has several.
    let first = if has_head {
        let roots = git(&root, &["rev-list", "--max-parents=0", "HEAD"])?;
        match roots.lines().map(str::trim).rfind(|sha| !sha.is_empty()) {
            Some(sha) => CommitInfo::all(&git(&root, &["log", "-1", &format!("--format={COMMIT_FORMAT}"), sha])?)
                .into_iter()
                .next(),
            None => None,
        }
    } else {
        None
    };
    let tag_count = refs.tags.len() as u32;
    Ok(Overview {
        name: Path::new(&root).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        branch: refs.branch.clone(),
        status: status(&root)?.unwrap_or_default(),
        commits,
        last: latest.first().cloned(),
        first,
        remotes: remotes(&root),
        branches: refs.branches,
        tags: refs.tags.into_iter().take(TAGS).collect(),
        tag_count,
        latest,
        readme: readme(&root),
        path: root,
    })
}

/// Each remote with its URL, as `remote.<name>.url` has it.
fn remotes(root: &str) -> Vec<RemoteInfo> {
    let listed = git(root, &["config", "--get-regexp", r"^remote\..*\.url$"]).unwrap_or_default();
    let mut out: Vec<RemoteInfo> = Vec::new();
    for line in listed.lines() {
        let Some((key, url)) = line.split_once(' ') else { continue };
        let Some(name) = key.strip_prefix("remote.").and_then(|rest| rest.strip_suffix(".url")) else { continue };
        if out.iter().any(|known| known.name == name) {
            continue;
        }
        out.push(RemoteInfo { name: name.to_owned(), url: url.trim().to_owned(), web: web_page(name, url) });
    }
    out.sort_by(|a, b| (a.name != "origin").cmp(&(b.name != "origin")).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Opens a page of one of the repository's remotes in the browser (its project page, issues, pull requests); any
/// other address is refused.
pub fn open_remote_page(path: &str, remote: &str, url: &str) -> Result<(), String> {
    let root = repository(path)?;
    let found = remotes(&root)
        .into_iter()
        .find(|known| known.name == remote)
        .ok_or_else(|| said!("overview.noRemote", "That remote is not there."))?;
    let page = found.web.ok_or_else(|| said!("overview.noPage", "That remote has no page on the web."))?;
    if url != page.base && !url.starts_with(&format!("{}/", page.base)) {
        return Err(said!("overview.otherPage", "Only pages of the repository's remotes can be opened."));
    }
    crate::editor::open_url(url)
}

const README_NAMES: [&str; 6] = ["readme.md", "readme.markdown", "readme", "readme.txt", "readme.rst", "readme.adoc"];

/// The README at the repository's top, as the working tree has it; a link that leads out of the repository is not read.
fn readme(root: &str) -> Option<Readme> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file() || kind.is_symlink()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| README_NAMES.contains(&name.to_lowercase().as_str()))
        .collect();
    names.sort_by_key(|name| README_NAMES.iter().position(|known| *known == name.to_lowercase()));
    let name = names.into_iter().next()?;
    let file = crate::paths::inside(root, &name)?;
    let bytes = std::fs::read(file).ok()?;
    let cut = bytes.len() > README_BYTES;
    let mut text = String::from_utf8_lossy(&bytes[..bytes.len().min(README_BYTES)]).into_owned();
    if cut {
        // The last line may have been cut in two.
        if let Some(at) = text.rfind('\n') {
            text.truncate(at);
        }
    }
    Some(Readme { name, text, cut })
}

pub fn statistics(path: &str) -> Result<Statistics, String> {
    let root = repository(path)?;
    let has_head =
        git(&root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).is_ok_and(|sha| !sha.trim().is_empty());
    let mut authors = Vec::new();
    let mut months = Vec::new();
    if has_head {
        // "  12\tName <email>"
        let listed = git(&root, &["shortlog", "--summary", "--numbered", "--email", "HEAD"])?;
        for line in listed.lines().take(10) {
            let Some((count, who)) = line.trim().split_once('\t') else { continue };
            let (name, email) = match who.rsplit_once(" <") {
                Some((name, email)) => (name.to_owned(), email.trim_end_matches('>').to_owned()),
                None => (who.to_owned(), String::new()),
            };
            authors.push(AuthorCount { name, email, commits: count.trim().parse().unwrap_or(0) });
        }
        let dates = git(&root, &["log", "--since=12.months.ago", "--format=%ad", "--date=format:%Y-%m", "HEAD"])?;
        months = last_year(&dates, this_month());
    }
    let tracked = git(&root, &["ls-files", "-z"])?;
    let mut files = 0;
    let mut by_extension: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for path in tracked.split('\0').filter(|path| !path.is_empty()) {
        files += 1;
        let name = path.rsplit('/').next().unwrap_or(path);
        let extension = match name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => extension.to_lowercase(),
            _ => String::new(),
        };
        *by_extension.entry(extension).or_default() += 1;
    }
    let mut extensions: Vec<ExtensionCount> =
        by_extension.into_iter().map(|(extension, files)| ExtensionCount { extension, files }).collect();
    extensions.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.extension.cmp(&b.extension)));
    extensions.truncate(12);
    Ok(Statistics { authors, months, files, extensions })
}

/// This month as a year and a month, in UTC.
fn this_month() -> (i32, u32) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    // Days since 1970 to a civil date (Howard Hinnant's days_from_civil, backwards).
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month as u32)
}

/// The twelve months up to `now`, oldest first, each with the commits `dates` ("2026-10" a line) has in it.
fn last_year(dates: &str, now: (i32, u32)) -> Vec<MonthCount> {
    let (mut year, mut month) = now;
    let mut out = Vec::new();
    for _ in 0..12 {
        let name = format!("{year:04}-{month:02}");
        let commits = dates.lines().filter(|line| line.trim() == name).count() as u32;
        out.push(MonthCount { month: name, commits });
        if month == 1 {
            month = 12;
            year -= 1;
        } else {
            month -= 1;
        }
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_commits_of_each_month_of_the_last_year() {
        let months = last_year("2026-10\n2026-10\n2026-01\n2025-11\n2025-10\n", (2026, 10));
        assert_eq!(months.len(), 12);
        assert_eq!(months.first().map(|month| month.month.as_str()), Some("2025-11"));
        assert_eq!(months.last().map(|month| (month.month.as_str(), month.commits)), Some(("2026-10", 2)));
        assert_eq!(months.iter().find(|month| month.month == "2026-01").map(|month| month.commits), Some(1));
        assert_eq!(months.iter().map(|month| month.commits).sum::<u32>(), 4);
    }

    #[test]
    fn this_month_is_a_real_month() {
        let (year, month) = this_month();
        assert!(year >= 2024 && (1..=12).contains(&month));
    }
}
