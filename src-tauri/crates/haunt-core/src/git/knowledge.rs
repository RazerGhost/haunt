//! Who knows the code: who wrote the lines a set of files has now (`git blame` at HEAD, file by file, several at a
//! time), who changed each file lately (one `git log --numstat`), and who wrote the lines a change touches, for
//! suggesting reviewers. Read-only, through the safe runner. The lines written are split three ways: yours alone, the
//! ones an AI co-wrote (their commit names one as co-author, `attribution.rs`), and everyone else's.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::cli::{git, revision};
use super::types::AuthorLines;

/// The most files blamed for one answer, and how many at a time.
const BLAMED_FILES: usize = 300;
const AT_ONCE: usize = 8;

/// One author's changes to one file in the period.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FileAuthor {
    pub path: String,
    pub author: String,
    pub commits: u32,
    /// Lines added and removed.
    pub lines: u32,
    /// Their latest change to it, in seconds.
    pub last: i64,
}

/// Who changed which file in the last `days` days, per file and author. Paths are relative to the opened folder.
pub fn recent_authors(root: &str, days: u32) -> Result<Vec<FileAuthor>, String> {
    let since = format!("--since={}.days", days.max(1));
    let out = git(root, &["log", &since, "--numstat", "--no-renames", "--relative", "--format=%x1e%an%x1f%at"])?;
    let mut found: HashMap<(String, String), FileAuthor> = HashMap::new();
    let (mut author, mut time) = (String::new(), 0i64);
    for line in out.lines() {
        if let Some(header) = line.strip_prefix('\x1e') {
            let (name, at) = header.split_once('\x1f').unwrap_or((header, "0"));
            author = name.to_owned();
            time = at.parse().unwrap_or(0);
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let number = |text: &str| if text == "-" { Some(0) } else { text.parse::<u32>().ok() };
        let (Some(added), Some(removed)) = (number(added), number(removed)) else { continue };
        let path = super::history::unquote(path);
        let entry = found.entry((path.clone(), author.clone())).or_insert(FileAuthor {
            path,
            author: author.clone(),
            commits: 0,
            lines: 0,
            last: 0,
        });
        entry.commits += 1;
        entry.lines += added + removed;
        entry.last = entry.last.max(time);
    }
    let mut out: Vec<FileAuthor> = found.into_values().collect();
    // Ties by the latest change, then by name, so the order is the same every time.
    out.sort_by(|a, b| {
        a.path.cmp(&b.path).then(b.lines.cmp(&a.lines)).then(b.last.cmp(&a.last)).then(a.author.cmp(&b.author))
    });
    Ok(out)
}

/// Who wrote the lines of a set of files as HEAD has them.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Blamed {
    /// Authors by lines, most first.
    pub authors: Vec<AuthorLines>,
    pub files: u32,
    /// Files left out: past the limit, or not in HEAD yet.
    pub skipped: u32,
    /// Your lines whose commit names no AI as co-author (you are git's `user.name` or `user.email`).
    pub you_alone: u32,
    /// Lines whose commit names an AI as co-author, whoever committed them.
    pub ai: u32,
    /// Everyone else's lines without an AI co-author, most first.
    pub others: Vec<AuthorLines>,
    /// Your name in git, when it is set.
    pub you: Option<String>,
}

/// Lines counted by the commit that wrote them and its author's name and email.
type ByCommit = HashMap<(String, String, String), u32>;

/// Lines per commit and author in `git blame --line-porcelain`'s answer (lines not committed yet left out).
fn count_by_commit(porcelain: &str) -> ByCommit {
    let mut out = ByCommit::new();
    let (mut sha, mut author, mut mail) = (String::new(), String::new(), String::new());
    for line in porcelain.lines() {
        if line.starts_with('\t') {
            // The line itself ends its block.
            if author != "Not Committed Yet" && !sha.is_empty() {
                *out.entry((sha.clone(), author.clone(), mail.clone())).or_insert(0) += 1;
            }
        } else if let Some(name) = line.strip_prefix("author-mail ") {
            mail = name.trim_matches(['<', '>']).to_owned();
        } else if let Some(name) = line.strip_prefix("author ") {
            name.clone_into(&mut author);
        } else if line.len() > 40
            && line.as_bytes()[40] == b' '
            && line[..40].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            line[..40].clone_into(&mut sha);
        }
    }
    out
}

/// The lines split three ways: yours alone, co-written with an AI, everyone else's.
fn split(root: &str, lines: &ByCommit) -> Blamed {
    let config = |key: &str| {
        git(root, &["config", key]).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
    };
    let (you, email) = (config("user.name"), config("user.email"));
    let shas: HashSet<String> = lines.keys().map(|(sha, _, _)| sha.clone()).collect();
    let helped = super::attribution::ai_commits(root, &shas, &super::attribution::ai_patterns(root));
    let (mut authors, mut others): (HashMap<String, u32>, HashMap<String, u32>) = (HashMap::new(), HashMap::new());
    let (mut you_alone, mut ai) = (0, 0);
    for ((sha, author, mail), count) in lines {
        *authors.entry(author.clone()).or_insert(0) += count;
        let yours = you.as_deref() == Some(author.as_str())
            || email.as_deref().is_some_and(|email| email.eq_ignore_ascii_case(mail));
        if helped.contains(sha) {
            ai += count;
        } else if yours {
            you_alone += count;
        } else {
            *others.entry(author.clone()).or_insert(0) += count;
        }
    }
    Blamed { authors: sorted(authors), files: 0, skipped: 0, you_alone, ai, others: sorted(others), you }
}

/// Who wrote lines `line` to `end_line` of the file at `path` as it is now (one function), split the same way; lines
/// not committed yet are left out.
pub fn blame_span(root: &str, path: &str, line: u32, end_line: u32) -> Result<Blamed, String> {
    let range = format!("-L{},{}", line.max(1), end_line.max(line.max(1)));
    let out = git(root, &["blame", "-w", "--line-porcelain", &range, "--", path])?;
    Ok(Blamed { files: 1, ..split(root, &count_by_commit(&out)) })
}

/// Who wrote the lines of `paths` (relative to the opened folder) at HEAD, most lines first; at most
/// `BLAMED_FILES` files.
pub fn blame_authors(root: &str, paths: &[String]) -> Blamed {
    let wanted: Vec<&String> = paths.iter().take(BLAMED_FILES).collect();
    let mut skipped = paths.len().saturating_sub(wanted.len()) as u32;
    let mut lines = ByCommit::new();
    let mut files = 0;
    for chunk in wanted.chunks(AT_ONCE) {
        let results: Vec<Option<ByCommit>> = std::thread::scope(|scope| {
            let running: Vec<_> = chunk
                .iter()
                .map(|path| {
                    scope.spawn(move || {
                        git(root, &["blame", "-w", "--line-porcelain", "HEAD", "--", path])
                            .ok()
                            .map(|out| count_by_commit(&out))
                    })
                })
                .collect();
            running.into_iter().map(|run| run.join().ok().flatten()).collect()
        });
        for result in results {
            match result {
                Some(found) => {
                    files += 1;
                    for (key, count) in found {
                        *lines.entry(key).or_insert(0) += count;
                    }
                }
                None => skipped += 1,
            }
        }
    }
    Blamed { files, skipped, ..split(root, &lines) }
}

/// A part of a file in a version: for who wrote it.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineSpanAt {
    pub path: String,
    pub line: u32,
    pub end_line: u32,
}

/// Who wrote the lines of `spans` at commit `base` (the lines a change touches, as they were), most lines first; and
/// your own name in git, to leave yourself out.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Knowers {
    pub authors: Vec<AuthorLines>,
    pub you: Option<String>,
}

pub fn reviewers(root: &str, base: &str, spans: &[LineSpanAt]) -> Result<Knowers, String> {
    let base = revision(base)?;
    let mut lines: HashMap<String, u32> = HashMap::new();
    for chunk in spans.iter().take(200).collect::<Vec<_>>().chunks(AT_ONCE) {
        let results: Vec<Option<HashMap<String, u32>>> = std::thread::scope(|scope| {
            let running: Vec<_> = chunk
                .iter()
                .map(|span| {
                    scope.spawn(move || {
                        let range = format!("-L{},{}", span.line.max(1), span.end_line.max(span.line.max(1)));
                        git(root, &["blame", "-w", "--line-porcelain", &range, base, "--", &span.path])
                            .ok()
                            .map(|out| count(&out))
                    })
                })
                .collect();
            running.into_iter().map(|run| run.join().ok().flatten()).collect()
        });
        for found in results.into_iter().flatten() {
            for (author, count) in found {
                *lines.entry(author).or_insert(0) += count;
            }
        }
    }
    let you =
        git(root, &["config", "user.name"]).ok().map(|name| name.trim().to_owned()).filter(|name| !name.is_empty());
    Ok(Knowers { authors: sorted(lines), you })
}

/// Lines per author in `git blame --line-porcelain`'s answer (lines not committed yet left out).
fn count(porcelain: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for line in porcelain.lines() {
        if let Some(author) = line.strip_prefix("author ") {
            if author != "Not Committed Yet" {
                *out.entry(author.to_owned()).or_insert(0) += 1;
            }
        }
    }
    out
}

fn sorted(lines: HashMap<String, u32>) -> Vec<AuthorLines> {
    let mut out: Vec<AuthorLines> = lines.into_iter().map(|(name, lines)| AuthorLines { name, lines }).collect();
    out.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.name.cmp(&b.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Repo;

    #[test]
    fn counts_who_wrote_and_who_changed_and_who_knows_a_change() {
        let r = Repo::new();
        r.write("a.txt", "one\ntwo\nthree\n");
        r.write("b.txt", "x\n");
        r.sh(&["add", "."]);
        r.sh(&["-c", "user.name=Ann", "commit", "-q", "-m", "Ann writes"]);
        r.write("a.txt", "one\nTWO\nthree\nfour\n");
        r.sh(&["-c", "user.name=Bo", "commit", "-q", "-am", "Bo edits"]);
        let base = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();

        let blamed = blame_authors(&r.root, &["a.txt".into(), "b.txt".into(), "missing.txt".into()]);
        let seen: Vec<(&str, u32)> = blamed.authors.iter().map(|author| (author.name.as_str(), author.lines)).collect();
        assert_eq!(seen, [("Ann", 3), ("Bo", 2)]);
        assert_eq!((blamed.files, blamed.skipped), (2, 1));

        let recent = recent_authors(&r.root, 30).unwrap();
        let a: Vec<(&str, u32, u32)> = recent
            .iter()
            .filter(|item| item.path == "a.txt")
            .map(|item| (item.author.as_str(), item.commits, item.lines))
            .collect();
        assert_eq!(a, [("Bo", 1, 3), ("Ann", 1, 3)]);

        // Lines 1 to 2 of a.txt as they are at base: one by Ann, one by Bo; the repository's own user is Test.
        let knowers = reviewers(&r.root, &base, &[LineSpanAt { path: "a.txt".into(), line: 1, end_line: 2 }]).unwrap();
        let names: Vec<&str> = knowers.authors.iter().map(|author| author.name.as_str()).collect();
        assert_eq!(names, ["Ann", "Bo"]);
        assert_eq!(knowers.you.as_deref(), Some("Test"));
    }

    #[test]
    fn splits_the_lines_into_yours_alone_co_written_with_an_ai_and_others() {
        let r = Repo::new();
        r.write(
            "a.txt", "one
two
",
        );
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Test writes"]);
        r.write(
            "a.txt",
            "one
two
three
four
five
",
        );
        r.sh(&[
            "commit",
            "-q",
            "-am",
            "Test and Claude add

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
        ]);
        r.write(
            "a.txt",
            "one
two
three
four
five
six
",
        );
        r.sh(&["-c", "user.name=Ann", "-c", "user.email=ann@example.com", "commit", "-q", "-am", "Ann adds"]);
        r.write(
            "a.txt",
            "one
two
three
four
five
six
seven
",
        );

        let blamed = blame_authors(&r.root, &["a.txt".into()]);
        assert_eq!((blamed.you_alone, blamed.ai), (2, 3));
        let others: Vec<(&str, u32)> =
            blamed.others.iter().map(|author| (author.name.as_str(), author.lines)).collect();
        assert_eq!(others, [("Ann", 1)]);
        assert_eq!(blamed.you.as_deref(), Some("Test"));

        // Lines 2 to 7 as the file is now: one of yours, three with Claude, Ann's, and one not committed yet.
        let span = blame_span(&r.root, "a.txt", 2, 7).unwrap();
        assert_eq!((span.you_alone, span.ai, span.others.len()), (1, 3, 1));
        assert_eq!(span.authors.iter().map(|author| author.lines).sum::<u32>(), 5);
    }
}
