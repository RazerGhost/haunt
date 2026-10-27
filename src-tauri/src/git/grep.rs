//! Searching the files of a commit for a text: `git grep` on the commit's tree, read only. It searches the opened
//! folder's part of the repository and names files from that folder, as the window does everywhere. The repository's
//! own grep settings cannot change what comes back: every one of them is set on the command line.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::cli::{git, revision, stream};

/// At most this many matching lines are read, in at most this many files.
const MOST_LINES: usize = 2000;
const MOST_FILES: usize = 200;
/// A matching line is cut after this many characters: a minified file is one very long line.
const LINE_CHARS: usize = 300;

/// How the pattern is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum GrepMode {
    /// The text as it is.
    Text,
    /// An extended regular expression, as `grep -E` reads it.
    Regex,
    /// The text as it is, as a whole word.
    Word,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrepResult {
    /// The commit searched, as a full sha.
    pub at: String,
    pub files: Vec<GrepFile>,
    /// There were more matches than these.
    pub cut: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrepFile {
    /// Relative to the opened folder.
    pub path: String,
    pub matches: Vec<GrepLine>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrepLine {
    /// From 1.
    pub line: u32,
    /// The line, cut after `LINE_CHARS` characters.
    pub text: String,
}

/// The lines of the files at commit `at` that hold `pattern`, by file, limited to `path` (a folder, file or glob) when
/// given. Binary files are skipped.
pub fn grep(
    root: &str,
    at: &str,
    pattern: &str,
    mode: GrepMode,
    ignore_case: bool,
    path: Option<&str>,
) -> Result<GrepResult, String> {
    if pattern.is_empty() {
        return Err(said!("grep.empty", "Type something to search for."));
    }
    let at = revision(at)?;
    let spec = format!("{at}^{{commit}}");
    let sha = git(root, &["rev-parse", "--verify", "--quiet", "--end-of-options", &spec])
        .map(|out| out.trim().to_owned())
        .ok()
        .filter(|sha| !sha.is_empty())
        .ok_or_else(|| said!("grep.notCommit", "That is not a commit."))?;
    let path = path.map(str::trim).filter(|path| !path.is_empty()).map(pathspec).transpose()?;

    // Every setting a repository's grep config could change is given here: how the pattern is read, the line numbers,
    // the paths from the opened folder, no columns and no submodules.
    let mut args = vec![
        "grep",
        "--no-color",
        "-I",
        "-n",
        "--null",
        "--no-column",
        "--no-full-name",
        "--no-recurse-submodules",
        "--no-textconv",
    ];
    args.extend(match mode {
        GrepMode::Text => &["--fixed-strings"][..],
        GrepMode::Regex => &["--extended-regexp"][..],
        GrepMode::Word => &["--fixed-strings", "--word-regexp"][..],
    });
    if ignore_case {
        args.push("-i");
    }
    args.extend(["-e", pattern, &sha, "--"]);
    if let Some(path) = path {
        args.push(path);
    }

    let mut found = GrepResult { at: sha.clone(), files: Vec::new(), cut: false };
    let mut lines = stream(root, &args)?;
    let mut count = 0;
    // A file name may hold a line end: such a line is taken with the next one until it has its two NULs.
    let mut pending = String::new();
    while let Some(line) = lines.next_line() {
        let line = match line {
            Ok(line) => line,
            // git grep says nothing and stops with 1 when nothing matches.
            Err(message) if is_no_match(&message) => break,
            Err(message) => return Err(message),
        };
        if !pending.is_empty() {
            pending.push('\n');
        }
        pending.push_str(&line);
        let Some((file, number, text)) = parse(&pending, &sha) else { continue };
        pending.clear();
        if count == MOST_LINES {
            found.cut = true;
            break;
        }
        if found.files.last().is_none_or(|last| last.path != file) {
            if found.files.len() == MOST_FILES {
                found.cut = true;
                break;
            }
            found.files.push(GrepFile { path: file, matches: Vec::new() });
        }
        if let Some(last) = found.files.last_mut() {
            last.matches.push(GrepLine { line: number, text });
        }
        count += 1;
    }
    Ok(found)
}

/// One match as `git grep --null` gives it for a commit: "sha:path NUL line NUL text".
/// git grep's "git failed (exit code: 1)": nothing matched.
fn is_no_match(message: &str) -> bool {
    let english = crate::said::english(message);
    english.starts_with("git failed (") && english.ends_with(": 1)")
}

fn parse(line: &str, sha: &str) -> Option<(String, u32, String)> {
    let mut parts = line.splitn(3, '\0');
    let named = parts.next()?;
    let number = parts.next()?.parse().ok()?;
    let text = parts.next()?;
    let path = named.strip_prefix(sha).and_then(|rest| rest.strip_prefix(':')).unwrap_or(named);
    let text = match text.char_indices().nth(LINE_CHARS) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    };
    Some((path.to_owned(), number, text))
}

/// A path filter from the window: inside the opened folder (relative, no "..") and never an option or a pathspec's
/// magic. A glob ("*.ts", "src/**/*.rs") is fine.
fn pathspec(path: &str) -> Result<&str, String> {
    let fine = !path.starts_with('-')
        && !path.starts_with(':')
        && !path.contains('\0')
        && Path::new(path).components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if fine {
        Ok(path)
    } else {
        Err(said!("grep.notPath", "\"{path}\" is not a path in this folder.", path = path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_match_and_cuts_long_lines() {
        let (path, line, text) = parse("abc:src/a:b.ts\x0012\x00let a = 1", "abc").unwrap();
        assert_eq!((path.as_str(), line, text.as_str()), ("src/a:b.ts", 12, "let a = 1"));
        let long = format!("abc:x\x001\x00{}", "é".repeat(400));
        assert_eq!(parse(&long, "abc").unwrap().2.chars().count(), LINE_CHARS);
        assert!(parse("abc:x\x001", "abc").is_none());
    }

    #[test]
    fn refuses_paths_outside_the_folder() {
        assert!(pathspec("src").is_ok());
        assert!(pathspec("*.ts").is_ok());
        assert!(pathspec("src/**/*.rs").is_ok());
        assert!(pathspec("../other").is_err());
        assert!(pathspec("src/../../x").is_err());
        assert!(pathspec("/etc").is_err());
        assert!(pathspec("--output=x").is_err());
        assert!(pathspec(":(top)x").is_err());
    }

    #[test]
    fn searches_a_commit() {
        let base = std::env::temp_dir().join(format!("haunt-grep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("src")).unwrap();
        let root = base.to_string_lossy().into_owned();
        let run = |args: &[&str]| {
            let mut command = std::process::Command::new("git");
            command
                .args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(&base);
            assert!(command.output().unwrap().status.success(), "{args:?}");
        };
        let write = |path: &str, text: &str| std::fs::write(base.join(path), text).unwrap();
        run(&["init", "-q", "-b", "main"]);
        write("src/old.ts", "const oldName = 1\n");
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "first"]);
        let first = git(&root, &["rev-parse", "HEAD"]).unwrap().trim().to_owned();
        std::fs::remove_file(base.join("src/old.ts")).unwrap();
        write("src/a.ts", "let foo = 1\nlet foobar = 2\nFOO()\n-flag here\n");
        write("notes.md", "foo in the notes\n");
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "second"]);

        let lines = |found: &GrepResult| -> Vec<(String, u32)> {
            found.files.iter().flat_map(|file| file.matches.iter().map(|hit| (file.path.clone(), hit.line))).collect()
        };
        let text = grep(&root, "HEAD", "foo", GrepMode::Text, false, None).unwrap();
        assert_eq!(text.at.len(), 40);
        assert_eq!(lines(&text), [("notes.md".into(), 1), ("src/a.ts".into(), 1), ("src/a.ts".into(), 2)]);
        assert!(!text.cut);
        assert_eq!(text.files[1].matches[1].text, "let foobar = 2");

        let word = grep(&root, "main", "foo", GrepMode::Word, false, Some("src")).unwrap();
        assert_eq!(lines(&word), [("src/a.ts".into(), 1)]);
        let case = grep(&root, "main", "foo", GrepMode::Word, true, Some("src")).unwrap();
        assert_eq!(lines(&case), [("src/a.ts".into(), 1), ("src/a.ts".into(), 3)]);
        // A dot is any character as a regex, and only a dot as text.
        let regex = grep(&root, "HEAD", "fo.bar", GrepMode::Regex, false, None).unwrap();
        assert_eq!(lines(&regex), [("src/a.ts".into(), 2)]);
        assert!(grep(&root, "HEAD", "fo.bar", GrepMode::Text, false, None).unwrap().files.is_empty());
        let glob = grep(&root, "HEAD", "foo", GrepMode::Text, false, Some("*.md")).unwrap();
        assert_eq!(lines(&glob), [("notes.md".into(), 1)]);

        let dash = grep(&root, "HEAD", "-flag", GrepMode::Text, false, None).unwrap();
        assert_eq!(lines(&dash), [("src/a.ts".into(), 4)]);
        let none = grep(&root, "HEAD", "nowhere at all", GrepMode::Text, false, None).unwrap();
        assert!(none.files.is_empty() && !none.cut);

        let old = grep(&root, &first[..10], "oldName", GrepMode::Text, false, None).unwrap();
        assert_eq!((old.at.as_str(), lines(&old)), (first.as_str(), vec![("src/old.ts".into(), 1)]));
        assert!(grep(&root, "HEAD", "oldName", GrepMode::Text, false, None).unwrap().files.is_empty());

        // From a folder deeper in, only its files, named from there.
        let inner = base.join("src").to_string_lossy().into_owned();
        let deeper = grep(&inner, "HEAD", "foo", GrepMode::Text, false, None).unwrap();
        assert_eq!(lines(&deeper), [("a.ts".into(), 1), ("a.ts".into(), 2)]);

        assert_eq!(
            crate::said::english(&grep(&root, "nothing-here", "foo", GrepMode::Text, false, None).unwrap_err()),
            "That is not a commit."
        );
        assert!(grep(&root, "--all", "foo", GrepMode::Text, false, None).is_err());
        assert!(grep(&root, "HEAD", "", GrepMode::Text, false, None).is_err());
        assert!(grep(&root, "HEAD", "foo", GrepMode::Text, false, Some("../x")).is_err());
        assert!(grep(&root, "HEAD", "(", GrepMode::Regex, false, None).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
