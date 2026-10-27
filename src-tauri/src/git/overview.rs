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
    // Each part is a git call or two, or a read in the process: read side by side. The history is read before it is
    // known whether HEAD has a commit, and left out after when it has none.
    let (refs, status, history, remotes, readme) = std::thread::scope(|scope| {
        let status = scope.spawn(|| status(&root));
        let history = scope.spawn(|| history(&root));
        let remotes = scope.spawn(|| remotes(&root));
        let readme = scope.spawn(|| readme(&root));
        let refs = super::refs(&root);
        (refs, joined(status), joined(history), joined(remotes), joined(readme))
    });
    let refs = refs?.ok_or_else(|| said!("overview.notRepository", "That folder is not a git repository."))?;
    let History { commits, latest, first } = if refs.sha.is_some() { history? } else { History::default() };
    let tag_count = refs.tags.len() as u32;
    Ok(Overview {
        name: Path::new(&root).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        branch: refs.branch.clone(),
        status: status?.unwrap_or_default(),
        commits,
        last: latest.first().cloned(),
        first,
        remotes,
        branches: refs.branches,
        tags: refs.tags.into_iter().take(TAGS).collect(),
        tag_count,
        latest,
        readme,
        path: root,
    })
}

/// A scoped thread's result; its panic goes on as it would have without the thread.
fn joined<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// HEAD's history for the overview.
#[derive(Debug, Default)]
struct History {
    /// Commits reachable from HEAD.
    commits: u32,
    /// The latest `LATEST`, newest first.
    latest: Vec<CommitInfo>,
    /// The oldest root commit: a history that joined another one has several.
    first: Option<CommitInfo>,
}

/// HEAD's history, read in the process with gix, or by git when gix leaves it to git.
fn history(root: &str) -> Result<History, String> {
    match history_in_process(root) {
        Some(history) => Ok(history),
        None => history_by_git(root),
    }
}

/// HEAD's history as git reads it: four calls.
fn history_by_git(root: &str) -> Result<History, String> {
    let commits = git(root, &["rev-list", "--count", "HEAD"])?.trim().parse().unwrap_or(0);
    let latest = CommitInfo::all(&git(
        root,
        &["log", &format!("--max-count={LATEST}"), &format!("--format={COMMIT_FORMAT}"), "HEAD"],
    )?);
    let roots = git(root, &["rev-list", "--max-parents=0", "HEAD"])?;
    let first = match roots.lines().map(str::trim).rfind(|sha| !sha.is_empty()) {
        Some(sha) => {
            CommitInfo::all(&git(root, &["log", "-1", &format!("--format={COMMIT_FORMAT}"), sha])?).into_iter().next()
        }
        None => None,
    };
    Ok(History { commits, latest, first })
}

/// HEAD's history read in the process, walked as git walks it: newest commit date first, a tie in the order the
/// commits were met, each parent once. Commit dates and parents come from the commit-graph when it has them, so only
/// the commits shown are read whole. None leaves it to git: a repository gix cannot open, a shallow one or one with
/// grafts (git's history there is not the objects'), a log encoding other than UTF-8, or a commit read here
/// differently than git might.
fn history_in_process(root: &str) -> Option<History> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashSet};

    let repo = gix::open(root).ok()?;
    if repo.is_shallow().unwrap_or(true) || repo.common_dir().join("info").join("grafts").exists() {
        return None;
    }
    let config = repo.config_snapshot();
    for key in ["i18n.logOutputEncoding", "i18n.commitEncoding"] {
        if config.string(key).is_some_and(|encoding| !utf8(encoding.as_ref())) {
            return None;
        }
    }
    let graph = repo.commit_graph_if_enabled().ok().flatten();
    let head = repo.head_commit().ok()?.id;

    let mut commits: u32 = 0;
    let mut latest = Vec::new();
    let mut first = None;
    let mut seen = HashSet::from([head]);
    let (time, parents) = links(&repo, graph.as_ref(), head)?;
    let mut met: u64 = 0;
    let mut queue = BinaryHeap::from([(time, Reverse(met), head, parents)]);
    while let Some((_, _, id, parents)) = queue.pop() {
        commits = commits.checked_add(1)?;
        if latest.len() < LATEST as usize {
            latest.push(id);
        }
        if parents.is_empty() {
            first = Some(id);
        }
        for parent in parents {
            if seen.insert(parent) {
                let (time, grandparents) = links(&repo, graph.as_ref(), parent)?;
                met += 1;
                queue.push((time, Reverse(met), parent, grandparents));
            }
        }
    }
    let latest = latest.into_iter().map(|id| commit_info(&repo, id)).collect::<Option<Vec<_>>>()?;
    let first = match first {
        Some(id) => Some(commit_info(&repo, id)?),
        None => None,
    };
    Some(History { commits, latest, first })
}

/// Whether an encoding's name is UTF-8's, as git compares them.
fn utf8(name: &[u8]) -> bool {
    name.eq_ignore_ascii_case(b"utf-8") || name.eq_ignore_ascii_case(b"utf8")
}

/// git's whitespace.
fn space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// A commit's date (the committer's) and parents, from the commit-graph or else the commit itself.
fn links(
    repo: &gix::Repository,
    graph: Option<&gix::commitgraph::Graph>,
    id: gix::ObjectId,
) -> Option<(i64, Vec<gix::ObjectId>)> {
    if let Some((graph, commit)) = graph.and_then(|graph| Some((graph, graph.commit_by_id(id)?))) {
        let parents = commit
            .iter_parents()
            .map(|position| position.ok().map(|position| graph.commit_at(position).id().to_owned()))
            .collect::<Option<Vec<_>>>()?;
        return Some((i64::try_from(commit.committer_timestamp()).ok()?, parents));
    }
    let object = repo.find_object(id).ok()?;
    if object.kind != gix::object::Kind::Commit {
        return None;
    }
    let mut lines = object.data.split(|byte| *byte == b'\n');
    lines.next()?.strip_prefix(b"tree ")?;
    let mut parents = Vec::new();
    let mut line = lines.next()?;
    while let Some(hex) = line.strip_prefix(b"parent ") {
        parents.push(gix::ObjectId::from_hex(hex).ok()?);
        line = lines.next()?;
    }
    // As git's parse_commit_date: the author's line, then the committer's, its date after the first '>'.
    if !line.starts_with(b"author") {
        return None;
    }
    let committer = lines.next()?.strip_prefix(b"committer")?;
    let after = &committer[committer.iter().position(|byte| *byte == b'>')? + 1..];
    Some((digits(after)?.parse().ok()?, parents))
}

/// The digits a date starts with, after any whitespace; None when there are none.
fn digits(text: &[u8]) -> Option<&str> {
    let text = &text[text.iter().take_while(|byte| space(**byte)).count()..];
    let length = text.iter().take_while(|byte| byte.is_ascii_digit()).count();
    (length > 0).then(|| std::str::from_utf8(&text[..length]).ok()).flatten()
}

/// A commit as `git log --format=COMMIT_FORMAT` gives it: the line is put together as git would put it, then read the
/// same way.
fn commit_info(repo: &gix::Repository, id: gix::ObjectId) -> Option<CommitInfo> {
    let object = repo.find_object(id).ok()?;
    if object.kind != gix::object::Kind::Commit {
        return None;
    }
    // git reads a commit as a C string.
    let data = object.data.split(|byte| *byte == 0).next().unwrap_or_default();
    let (headers, message) = match data.windows(2).position(|pair| pair == b"\n\n") {
        Some(at) => (&data[..at], &data[at + 2..]),
        None => (data, &data[data.len()..]),
    };
    let mut author = None;
    for line in headers.split(|byte| *byte == b'\n') {
        if let Some(value) = line.strip_prefix(b"author ") {
            author = Some(value);
        } else if line.strip_prefix(b"encoding ").is_some_and(|encoding| !utf8(encoding)) {
            return None;
        }
    }
    // As git's split_ident_line: the name up to the first '<', its date after the last '>'.
    let author = author?;
    let open = author.iter().position(|byte| *byte == b'<')?;
    let name = &author[..open];
    let name = &name[..name.len() - name.iter().rev().take_while(|byte| space(**byte)).count()];
    author[open..].iter().position(|byte| *byte == b'>')?;
    let time = digits(&author[author.iter().rposition(|byte| *byte == b'>')? + 1..])?;
    let mut line = format!("{id}\x1f").into_bytes();
    line.extend_from_slice(name);
    line.extend_from_slice(format!("\x1f{time}\x1f").as_bytes());
    line.extend_from_slice(&subject(message));
    CommitInfo::parse(&String::from_utf8_lossy(&line))
}

/// `%s` as git makes it: the first paragraph after any blank lines, its lines' ends trimmed and joined by a space.
fn subject(message: &[u8]) -> Vec<u8> {
    let blank = |line: &[u8]| line.iter().all(|byte| space(*byte));
    let mut out = Vec::new();
    let mut started = false;
    for line in message.split_inclusive(|byte| *byte == b'\n') {
        if blank(line) {
            if started {
                break;
            }
            continue;
        }
        if started {
            out.push(b' ');
        }
        started = true;
        out.extend_from_slice(&line[..line.len() - line.iter().rev().take_while(|byte| space(**byte)).count()]);
    }
    out
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
    // Four git calls, side by side; the two on HEAD's history are left out after when HEAD has no commit.
    let (has_head, listed, dates, tracked) = std::thread::scope(|scope| {
        let listed = scope.spawn(|| git(&root, &["shortlog", "--summary", "--numbered", "--email", "HEAD"]));
        let dates = scope
            .spawn(|| git(&root, &["log", "--since=12.months.ago", "--format=%ad", "--date=format:%Y-%m", "HEAD"]));
        let tracked = scope.spawn(|| git(&root, &["ls-files", "-z"]));
        let has_head =
            git(&root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).is_ok_and(|sha| !sha.trim().is_empty());
        (has_head, joined(listed), joined(dates), joined(tracked))
    });
    let mut authors = Vec::new();
    let mut months = Vec::new();
    if has_head {
        // "  12\tName <email>"
        let listed = listed?;
        for line in listed.lines().take(10) {
            let Some((count, who)) = line.trim().split_once('\t') else { continue };
            let (name, email) = match who.rsplit_once(" <") {
                Some((name, email)) => (name.to_owned(), email.trim_end_matches('>').to_owned()),
                None => (who.to_owned(), String::new()),
            };
            authors.push(AuthorCount { name, email, commits: count.trim().parse().unwrap_or(0) });
        }
        months = last_year(&dates?, this_month());
    }
    let tracked = tracked?;
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

    /// A history as text, to compare.
    fn seen<E: std::fmt::Debug>(history: Result<History, E>) -> String {
        format!("{history:?}")
    }

    /// git in the test repository with every date `date`: commits made in one second tie in the walk.
    fn at(repo: &crate::git::tests::Repo, date: &str, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo.root)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// A history with ties, merges, two roots, over `LATEST` commits and an odd message.
    fn tangled() -> crate::git::tests::Repo {
        let r = crate::git::tests::Repo::new();
        let tie = "@1700000000 +0000";
        r.write("a.txt", "a");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Root"]);
        at(&r, tie, &["switch", "-q", "-c", "side"]);
        for n in 0..6 {
            at(&r, tie, &["commit", "-q", "--allow-empty", "-m", &format!("Side {n}")]);
        }
        at(&r, tie, &["switch", "-q", "main"]);
        for n in 0..18 {
            r.write("a.txt", &n.to_string());
            at(&r, tie, &["commit", "-q", "-am", &format!("Main {n}")]);
        }
        at(&r, tie, &["merge", "-q", "--no-ff", "-m", "Merge side", "side"]);
        at(&r, "@1600000000 +0000", &["switch", "-q", "--orphan", "other"]);
        r.write("b.txt", "b");
        at(&r, "@1600000000 +0000", &["add", "."]);
        at(&r, "@1600000000 +0000", &["commit", "-q", "-m", "Other root"]);
        at(&r, tie, &["switch", "-q", "main"]);
        at(&r, tie, &["merge", "-q", "--allow-unrelated-histories", "-m", "Join", "other"]);
        r.write("message.txt", "\n  \n  Title  one  \t\nand two \n\nBody\n");
        let tree = at(&r, tie, &["rev-parse", "HEAD^{tree}"]);
        let message = r.path("message.txt").to_string_lossy().into_owned();
        let odd = at(&r, tie, &["commit-tree", &tree, "-p", "HEAD", "-F", &message]);
        at(&r, tie, &["update-ref", "HEAD", &odd]);
        r
    }

    #[test]
    fn reads_the_history_as_git_does() {
        let r = tangled();
        let by_git = history_by_git(&r.root).unwrap();
        assert!(by_git.commits > LATEST);
        assert_eq!(by_git.latest.len(), LATEST as usize);
        assert_eq!(by_git.first.as_ref().map(|first| first.subject.as_str()), Some("Other root"));
        assert_eq!(by_git.latest[0].subject, "  Title  one and two");
        assert_eq!(seen(history_in_process(&r.root).ok_or("left to git")), seen::<&str>(Ok(by_git)));
        // The same from the commit-graph's dates and parents.
        r.sh(&["commit-graph", "write", "--reachable"]);
        assert_eq!(seen(history_in_process(&r.root).ok_or("left to git")), seen(history_by_git(&r.root)));
        assert_eq!(seen(history(&r.root)), seen(history_by_git(&r.root)));
    }

    #[test]
    fn leaves_to_git_what_git_reads_differently() {
        let r = tangled();
        r.sh(&["config", "i18n.logOutputEncoding", "ISO-8859-1"]);
        assert!(history_in_process(&r.root).is_none());
        assert_eq!(seen(history(&r.root)), seen(history_by_git(&r.root)));
        r.sh(&["config", "--unset", "i18n.logOutputEncoding"]);
        r.sh(&["config", "i18n.commitEncoding", "ISO-8859-1"]);
        r.sh(&["commit", "-q", "--allow-empty", "-m", "Encoded"]);
        r.sh(&["config", "--unset", "i18n.commitEncoding"]);
        assert!(history_in_process(&r.root).is_none());
        // A shallow clone's history ends where it was cut.
        let shallow = tempfile::tempdir().unwrap();
        let into = crate::paths::plain(std::fs::canonicalize(shallow.path()).unwrap()).to_string_lossy().into_owned();
        let source = format!("file:///{}", r.root.replace('\\', "/").trim_start_matches('/'));
        at(&r, "@1700000000 +0000", &["clone", "-q", "--depth", "3", &source, &into]);
        assert!(history_in_process(&into).is_none());
        let cut = history(&into).unwrap();
        assert!(cut.commits < history_by_git(&r.root).unwrap().commits);
        assert_eq!(cut.first.map(|first| first.subject), Some("Join".to_owned()));
    }

    #[test]
    fn an_empty_repository_has_no_history() {
        let r = crate::git::tests::Repo::new();
        let read = overview(&r.root).unwrap();
        assert_eq!((read.commits, read.latest.len(), read.first.is_none(), read.last.is_none()), (0, 0, true, true));
        let stats = statistics(&r.root).unwrap();
        assert!(stats.authors.is_empty() && stats.months.is_empty() && stats.files == 0);
    }

    #[test]
    fn the_overview_reads_every_part() {
        let r = tangled();
        let read = overview(&r.root).unwrap();
        let by_git = history_by_git(&r.root).unwrap();
        assert_eq!(read.branch.as_deref(), Some("main"));
        assert_eq!(
            format!("{:?}", (read.commits, &read.latest, &read.first, &read.last)),
            format!("{:?}", (by_git.commits, &by_git.latest, &by_git.first, by_git.latest.first()))
        );
        assert_eq!(read.status.unstaged.len(), 1, "message.txt is not tracked");
        let stats = statistics(&r.root).unwrap();
        assert_eq!(stats.files, 2);
    }

    /// How long the overview and the statistics take on this repository (`HAUNT_OVERVIEW` names another):
    /// `cargo test overview_timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn overview_timing() {
        let path = std::env::var("HAUNT_OVERVIEW").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_owned() + "/..");
        // The first of each reads git's filters and warms the disk cache.
        let (_, _) = (overview(&path), statistics(&path));
        for _ in 0..5 {
            let started = std::time::Instant::now();
            let read = overview(&path).unwrap();
            let middle = started.elapsed();
            let stats = statistics(&path).unwrap();
            println!("overview {middle:?}, statistics {:?}", started.elapsed() - middle);
            if std::env::var("HAUNT_OVERVIEW_DUMP").is_ok() {
                let mut read = read;
                read.status = LocalStatus::default();
                println!("{}", serde_json::to_string(&read).unwrap());
                println!("{}", serde_json::to_string(&stats).unwrap());
            }
        }
    }

    #[test]
    fn this_month_is_a_real_month() {
        let (year, month) = this_month();
        assert!(year >= 2024 && (1..=12).contains(&month));
    }
}
