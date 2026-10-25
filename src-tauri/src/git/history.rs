//! A function's history and the hotspots, through git: the commits that touched a range of lines (`git log -L`, which
//! gix does not have), who wrote them (`git blame`), and how often each file changed lately (`git log --numstat`).
//! Also a file's whole history, followed across renames (`git log --follow`), and blame for a whole file.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::cli::{git, revision, top};
use super::types::{
    AuthorLines, BlameLine, Churn, CommitInfo, FileBlame, FileHistory, FileVersion, FunctionHistory, PathStatus,
    COMMIT_FORMAT,
};

/// The most lines a blame view holds.
const BLAME_LINES: usize = 20_000;

const FIELD: char = '\x1f';

/// The most commits of a function's history read at a time (each part, when it was moved).
pub const HISTORY_COMMITS: usize = 30;

/// The commits that touched lines `start` to `end` of a file, newest first, following it across renames; from commit
/// `at` back, or from the working tree's HEAD. With each, whether its change there was only cosmetic (`cosmetic`).
pub fn line_history(root: &str, path: &str, start: u32, end: u32, at: Option<&str>) -> Result<Vec<LineCommit>, String> {
    let range = format!("-L{start},{end}:{path}");
    let count = format!("--max-count={HISTORY_COMMITS}");
    let format = format!("--format=%x1e{COMMIT_FORMAT}");
    let mut args = vec!["log", &range, &count, "--no-color", &format];
    let at = at.map(revision).transpose()?;
    if let Some(at) = at {
        args.push(at);
    }
    let out = git(root, &args)?;
    Ok(out
        .split('\x1e')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let (header, patch) = record.split_once('\n').unwrap_or((record, ""));
            let commit = CommitInfo::parse(header)?;
            Some(LineCommit { commit, cosmetic: cosmetic(patch) })
        })
        .collect())
}

/// A commit of a range's history, and whether its change there was only cosmetic.
pub struct LineCommit {
    pub commit: CommitInfo,
    pub cosmetic: bool,
}

/// Whether a patch only moves spacing, blank lines and comments around: the lines it removes and adds are the same once
/// those are left out. Such a change leaves what the code does as it was.
fn cosmetic(patch: &str) -> bool {
    let code = |line: &str| -> Option<String> {
        let text = line.trim();
        let comment = ["//", "/*", "*", "#", "<!--", "--"].iter().any(|start| text.starts_with(start));
        (!text.is_empty() && !comment).then(|| text.chars().filter(|c| !c.is_whitespace()).collect())
    };
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    let mut changed = false;
    for line in patch.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            changed = true;
            removed.extend(code(rest));
        } else if let Some(rest) = line.strip_prefix('+') {
            changed = true;
            added.extend(code(rest));
        }
    }
    removed.sort();
    added.sort();
    changed && removed == added
}

/// Who wrote lines `start` to `end` of a file as it is now, by lines each, and the newest commit among them
/// (uncommitted lines count as now, by "You").
fn blame(root: &str, path: &str, start: u32, end: u32) -> Result<(Vec<AuthorLines>, Option<CommitInfo>), String> {
    let range = format!("-L{start},{end}");
    let out = git(root, &["blame", "--line-porcelain", &range, "--", path])?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_secs() as i64).unwrap_or(0);
    let mut authors: Vec<AuthorLines> = Vec::new();
    let mut latest: Option<CommitInfo> = None;
    let mut entry = CommitInfo { sha: String::new(), author: String::new(), time: 0, subject: String::new() };
    for line in out.lines() {
        let first = line.split(' ').next().unwrap_or_default();
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) && line.len() > 40 {
            entry = CommitInfo { sha: first.into(), author: String::new(), time: 0, subject: String::new() };
        } else if let Some(author) = line.strip_prefix("author ") {
            entry.author = author.into();
        } else if let Some(time) = line.strip_prefix("author-time ") {
            entry.time = time.parse().unwrap_or(0);
        } else if let Some(subject) = line.strip_prefix("summary ") {
            entry.subject = subject.into();
        } else if line.starts_with('\t') {
            let uncommitted = entry.sha.chars().all(|c| c == '0');
            let name = if uncommitted { "You (not committed yet)".to_owned() } else { entry.author.clone() };
            match authors.iter_mut().find(|known| known.name == name) {
                Some(known) => known.lines += 1,
                None => authors.push(AuthorLines { name: name.clone(), lines: 1 }),
            }
            let time = if uncommitted { now } else { entry.time };
            if latest.as_ref().is_none_or(|known| time > known.time) {
                latest =
                    Some(CommitInfo { sha: entry.sha.clone(), author: name, time, subject: entry.subject.clone() });
            }
        }
    }
    authors.sort_by_key(|author| std::cmp::Reverse(author.lines));
    Ok((authors, latest))
}

/// A function's history. A file git does not know yet (untracked) has none; that is not a problem.
pub fn function_history(root: &str, path: &str, start: u32, end: u32) -> FunctionHistory {
    let found = line_history(root, path, start, end, None).unwrap_or_default();
    let complete = found.len() < HISTORY_COMMITS;
    let cosmetic = found.iter().filter(|line| line.cosmetic).map(|line| line.commit.sha.clone()).collect();
    let commits = found.into_iter().map(|line| line.commit).collect();
    let (authors, latest) = blame(root, path, start, end).unwrap_or_default();
    FunctionHistory { commits, authors, latest, moves: Vec::new(), cosmetic, complete }
}

/// The last commit in which a file other than `path` gained or lost a mention of `name` (as a whole word): for a
/// function nothing uses now, about when its last use went (or, if it never had one, when its name was last written
/// elsewhere). None when no commit did.
pub fn last_mention(root: &str, name: &str, path: &str) -> Result<Option<CommitInfo>, String> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return Err(format!("{name} is not a name Haunt can look for."));
    }
    let word = format!("(^|[^A-Za-z0-9_$]){}([^A-Za-z0-9_$]|$)", name.replace('$', r"\$"));
    let pick = format!("-S{word}");
    let exclude = format!(":(exclude){path}");
    let out = git(
        root,
        &["log", "--max-count=1", "--pickaxe-regex", &pick, &format!("--format={COMMIT_FORMAT}"), "--", ".", &exclude],
    )?;
    Ok(out.lines().find_map(CommitInfo::parse))
}

/// The last `limit` commits along HEAD's first parents, newest first.
pub fn first_parents(root: &str, limit: u32) -> Result<Vec<CommitInfo>, String> {
    let count = format!("--max-count={limit}");
    let out = git(root, &["log", "--first-parent", &count, &format!("--format={COMMIT_FORMAT}"), "HEAD"])?;
    Ok(CommitInfo::all(&out))
}

/// A commit along HEAD's first parents, with its first parent and the files it changed against it.
#[derive(Debug, Clone)]
pub struct FirstParentCommit {
    pub info: CommitInfo,
    /// None for a first commit.
    pub parent: Option<String>,
    /// Each changed file: its status, its path before and after (the same unless renamed).
    pub files: Vec<(super::diff::FileStatus, String, String)>,
}

/// The last `limit` commits along HEAD's first parents, each with its parent and changed files, in one `git log`.
pub fn first_parent_changes(root: &str, limit: u32) -> Result<Vec<FirstParentCommit>, String> {
    use super::diff::FileStatus;
    let count = format!("--max-count={limit}");
    // -m with --first-parent: a merge's files are those it changed against its first parent.
    let out = git(
        root,
        &[
            "log",
            "--first-parent",
            "-m",
            &count,
            "--name-status",
            "-M",
            "--relative",
            "--format=%x1e%H%x1f%P%x1f%an%x1f%at%x1f%s",
            "HEAD",
            "--",
        ],
    )?;
    Ok(out
        .split('\x1e')
        .filter_map(|record| {
            let mut lines = record.lines();
            let mut parts = lines.next()?.split(FIELD);
            let mut next = || parts.next().unwrap_or_default().to_owned();
            let (sha, parents, author, time, subject) = (next(), next(), next(), next(), next());
            if sha.is_empty() {
                return None;
            }
            let files = lines
                .filter_map(|line| {
                    let mut fields = line.split('\t');
                    let status = fields.next()?;
                    let first = fields.next()?.to_owned();
                    let (status, old, new) = match status.chars().next()? {
                        'A' => (FileStatus::Added, first.clone(), first),
                        'D' => (FileStatus::Deleted, first.clone(), first),
                        'R' => (FileStatus::Renamed, first, fields.next()?.to_owned()),
                        _ => (FileStatus::Modified, first.clone(), first),
                    };
                    Some((status, old, new))
                })
                .collect();
            Some(FirstParentCommit {
                info: CommitInfo { sha, author, time: time.parse().unwrap_or(0), subject },
                parent: parents.split_whitespace().next().map(str::to_owned),
                files,
            })
        })
        .collect())
}

/// A commit's first parent and how many files it changed against it; None for a first commit.
pub fn parent_and_size(root: &str, sha: &str) -> Option<(String, usize)> {
    let sha = revision(sha).ok()?;
    let parent = git(root, &["rev-parse", "--verify", "--quiet", &format!("{sha}^")]).ok()?.trim().to_owned();
    if parent.is_empty() {
        return None;
    }
    let files = git(root, &["diff", "--name-only", "--no-renames", &parent, sha]).ok()?.lines().count();
    Some((parent, files))
}

/// A page of a file's history, newest first, following it across renames. Paths are relative to the opened folder.
pub fn file_history(root: &str, path: &str, skip: u32, limit: u32) -> Result<FileHistory, String> {
    let (_, prefix) = top(root)?;
    let limit = limit.clamp(1, 1000);
    // `--skip` does not work with `--follow` (git applies it before following), so the page is cut here.
    let count = format!("--max-count={}", skip + limit + 1);
    let out = git(
        root,
        &["log", "--follow", "--name-status", &count, "--format=%x1e%H%x1f%P%x1f%an%x1f%at%x1f%s", "--", path],
    )?;
    let inside = |path: &str| {
        let path = unquote(path);
        path.strip_prefix(prefix.as_str()).map(str::to_owned).unwrap_or(path)
    };
    let mut versions: Vec<FileVersion> = out
        .split('\x1e')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut lines = record.lines();
            let mut fields = lines.next()?.split(FIELD);
            let mut next = || fields.next().unwrap_or_default().to_owned();
            let (sha, parents, author, time, subject) = (next(), next(), next(), next(), next());
            // The status line: "M\tpath", or "R087\told\tnew" for a rename.
            let change = lines.find(|line| !line.trim().is_empty())?;
            let columns: Vec<&str> = change.split('\t').collect();
            let status = match columns.first()?.chars().next()? {
                'A' => PathStatus::Added,
                'D' => PathStatus::Deleted,
                'R' => PathStatus::Renamed,
                'C' => PathStatus::Copied,
                'T' => PathStatus::Type,
                _ => PathStatus::Modified,
            };
            let (old_path, path) = match columns.as_slice() {
                [_, old, new] => (inside(old), inside(new)),
                [_, one] => (inside(one), inside(one)),
                _ => return None,
            };
            Some(FileVersion {
                commit: CommitInfo { sha, author, time: time.parse().unwrap_or(0), subject },
                parent: parents.split(' ').find(|parent| !parent.is_empty()).map(str::to_owned),
                path,
                old_path,
                status,
            })
        })
        .skip(skip as usize)
        .collect();
    let more = versions.len() > limit as usize;
    versions.truncate(limit as usize);
    Ok(FileHistory { versions, more })
}

/// Who changed each line of a file last: as it is in the working tree (changes not committed yet count as "0000…"),
/// or at commit `at`.
pub fn file_blame(root: &str, path: &str, at: Option<&str>) -> Result<FileBlame, String> {
    let mut args = vec!["blame", "--porcelain"];
    let at = at.map(revision).transpose()?;
    if let Some(at) = at {
        args.push(at);
    }
    args.extend(["--", path]);
    let out = git(root, &args)?;
    let mut lines: Vec<BlameLine> = Vec::new();
    let mut commits: Vec<CommitInfo> = Vec::new();
    let mut known = std::collections::HashSet::new();
    let mut sha = String::new();
    let mut cut = false;
    for line in out.lines() {
        if let Some(text) = line.strip_prefix('\t') {
            if lines.len() >= BLAME_LINES {
                cut = true;
                break;
            }
            lines.push(BlameLine { sha: sha.clone(), text: text.to_owned() });
            continue;
        }
        let first = line.split(' ').next().unwrap_or_default();
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            sha = first.to_owned();
            if known.insert(sha.clone()) {
                commits.push(CommitInfo { sha: sha.clone(), author: String::new(), time: 0, subject: String::new() });
            }
        } else if let Some(commit) = commits.iter_mut().rev().find(|commit| commit.sha == sha) {
            if let Some(name) = line.strip_prefix("author ") {
                commit.author = name.to_owned();
            } else if let Some(time) = line.strip_prefix("author-time ") {
                commit.time = time.parse().unwrap_or(0);
            } else if let Some(subject) = line.strip_prefix("summary ") {
                commit.subject = subject.to_owned();
            }
        }
    }
    Ok(FileBlame { lines, commits, cut })
}

/// A path git prints in quotes when it has unusual characters ("a\303\251.ts").
pub(super) fn unquote(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else { return path.to_owned() };
    let mut bytes = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buffer = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some(digit @ '0'..='7') => {
                let mut value = digit.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    if let Some(next) = chars.peek().and_then(|c| c.to_digit(8)) {
                        value = value * 8 + next;
                        chars.next();
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// How much each file changed in the last `days` days, by commits and lines (current paths, renames not followed).
pub fn churn(root: &str, days: u32) -> Result<BTreeMap<String, Churn>, String> {
    let since = format!("--since={}.days", days.max(1));
    let out = git(root, &["log", &since, "--numstat", "--no-renames", "--relative", "--format=%x1e"])?;
    let mut files: BTreeMap<String, Churn> = BTreeMap::new();
    for line in out.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let number = |text: &str| if text == "-" { Some(0) } else { text.parse::<u32>().ok() };
        let (Some(added), Some(removed)) = (number(added), number(removed)) else { continue };
        let entry = files.entry(unquote(path)).or_default();
        entry.commits += 1;
        entry.lines += added + removed;
    }
    Ok(files)
}
