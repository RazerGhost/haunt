//! The local changes, as Local changes lists them: the files with staged changes and the ones with unstaged changes
//! (untracked files among them), and one file's diff on either side, as git reports it (so line endings and a
//! repository's attributes count as they do in a terminal). Read-only, through the safe runner; paths are relative to
//! the opened folder, as everywhere in Haunt.

use serde::Serialize;
use specta::Type;

use super::cli::{git, git_bytes, top};
use super::types::{DiffHunk, DiffLine, FileDiffView, LineKind, PathStatus};

/// A file with changes on one side.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    pub path: String,
    /// Where it was renamed from (the path itself otherwise).
    pub old_path: String,
    pub status: PathStatus,
    /// Not known to git yet (unstaged only).
    pub untracked: bool,
    /// Left with conflicts by a merge (unstaged only).
    pub conflicted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Type)]
pub struct LocalStatus {
    pub staged: Vec<LocalFile>,
    pub unstaged: Vec<LocalFile>,
}

fn status_of(code: char) -> PathStatus {
    match code {
        'A' => PathStatus::Added,
        'D' => PathStatus::Deleted,
        'R' => PathStatus::Renamed,
        'C' => PathStatus::Copied,
        'T' => PathStatus::Type,
        _ => PathStatus::Modified,
    }
}

/// The staged and unstaged files of the opened folder, or None when it is not in a git repository.
pub fn status(root: &str) -> Result<Option<LocalStatus>, String> {
    let Ok((_, prefix)) = top(root) else { return Ok(None) };
    // Porcelain v2 names every path from the repository's top; the whole repository is read, so a rename into the
    // opened folder from outside it is still seen as one, and paths outside the folder are left out below.
    let out = git(root, &["status", "--porcelain=v2", "-z", "--untracked-files=all", "--ignore-submodules=dirty"])?;
    let inside = |path: &str| path.strip_prefix(prefix.as_str()).map(str::to_owned);
    let mut result = LocalStatus::default();
    let mut tokens = out.split('\0').filter(|token| !token.is_empty());
    while let Some(entry) = tokens.next() {
        let mut fields = entry.splitn(2, ' ');
        let kind = fields.next().unwrap_or_default();
        let rest = fields.next().unwrap_or_default();
        match kind {
            "?" => {
                if let Some(path) = inside(rest) {
                    let file = LocalFile {
                        path: path.clone(),
                        old_path: path,
                        status: PathStatus::Added,
                        untracked: true,
                        conflicted: false,
                    };
                    result.unstaged.push(file);
                }
            }
            "1" | "2" | "u" => {
                // 1: XY sub mH mI mW hH hI path; 2: the same, a score, then the path, and the old path as the next
                // token; u: XY sub m1 m2 m3 mW h1 h2 h3 path.
                let skip = match kind {
                    "1" => 7,
                    "2" => 8,
                    _ => 9,
                };
                let parts: Vec<&str> = rest.splitn(skip + 1, ' ').collect();
                let (Some(xy), Some(path)) = (parts.first(), parts.get(skip)) else { continue };
                let old = if kind == "2" { tokens.next().unwrap_or(path) } else { path };
                let (Some(path), Some(old_path)) = (inside(path), inside(old).or_else(|| inside(path))) else {
                    continue;
                };
                let mut codes = xy.chars();
                let (x, y) = (codes.next().unwrap_or('.'), codes.next().unwrap_or('.'));
                if kind == "u" {
                    result.unstaged.push(LocalFile {
                        path: path.clone(),
                        old_path: path,
                        status: PathStatus::Modified,
                        untracked: false,
                        conflicted: true,
                    });
                    continue;
                }
                if x != '.' {
                    result.staged.push(LocalFile {
                        path: path.clone(),
                        old_path: old_path.clone(),
                        status: status_of(x),
                        untracked: false,
                        conflicted: false,
                    });
                }
                if y != '.' {
                    result.unstaged.push(LocalFile {
                        path: path.clone(),
                        old_path: path,
                        status: status_of(y),
                        untracked: false,
                        conflicted: false,
                    });
                }
            }
            _ => {}
        }
    }
    result.staged.sort_by(|a, b| a.path.cmp(&b.path));
    result.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(result))
}

/// A path from the window: relative to the opened folder, inside it, never an option.
pub fn checked(path: &str) -> Result<&str, String> {
    let bad = path.is_empty()
        || path.starts_with('-')
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..");
    if bad {
        return Err(format!("\"{path}\" is not a file in this folder."));
    }
    Ok(path)
}

/// Reads `git diff -U3` output into hunks with both sides' line numbers.
pub fn parse_hunks(text: &str) -> (bool, Vec<DiffHunk>) {
    let mut binary = false;
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let (mut old, mut new) = (0u32, 0u32);
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("Binary files ") || line == "GIT binary patch" {
            binary = true;
            continue;
        }
        if let Some(header) = line.strip_prefix("@@ -") {
            let mut parts = header.splitn(2, " @@");
            let ranges = parts.next().unwrap_or_default();
            let section = parts.next().unwrap_or_default().trim().to_owned();
            let mut sides = ranges.split(' ');
            let range = |text: Option<&str>| -> (u32, u32) {
                let text = text.unwrap_or_default().trim_start_matches('+');
                let (start, count) = text.split_once(',').unwrap_or((text, "1"));
                (start.parse().unwrap_or(0), count.parse().unwrap_or(1))
            };
            let (old_start, old_count) = range(sides.next());
            let (new_start, new_count) = range(sides.next());
            old = old_start;
            new = new_start;
            hunks.push(DiffHunk { old_start, old_count, new_start, new_count, section, lines: Vec::new() });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else { continue };
        let Some(first) = line.chars().next() else { continue };
        let text = line[first.len_utf8()..].to_owned();
        let kind = match first {
            ' ' => LineKind::Context,
            '+' => LineKind::Add,
            '-' => LineKind::Del,
            _ => continue,
        };
        let (on_old, on_new) = match kind {
            LineKind::Context => (Some(old), Some(new)),
            LineKind::Add => (None, Some(new)),
            LineKind::Del => (Some(old), None),
        };
        if on_old.is_some() {
            old += 1;
        }
        if on_new.is_some() {
            new += 1;
        }
        hunk.lines.push(DiffLine { kind, old: on_old, new: on_new, text });
    }
    (binary, hunks)
}

/// Whether git knows `path` (in the index), as opposed to an untracked file.
pub fn tracked(root: &str, path: &str) -> Result<bool, String> {
    Ok(!git(root, &["ls-files", "--", path])?.trim().is_empty())
}

/// Which of `paths` git tracks, in one call (each path taken as it is written, not as a pattern).
pub fn tracked_of(root: &str, paths: &[&str]) -> Result<std::collections::HashSet<String>, String> {
    if paths.is_empty() {
        return Ok(Default::default());
    }
    let literal: Vec<String> = paths.iter().map(|path| format!(":(literal){path}")).collect();
    let mut args = vec!["ls-files", "-z", "--"];
    args.extend(literal.iter().map(String::as_str));
    Ok(git(root, &args)?.split('\0').filter(|path| !path.is_empty()).map(str::to_owned).collect())
}

/// One file's local diff: the unstaged side (the index against the working tree, or a whole untracked file), or the
/// staged side (HEAD against the index). `old` is where a staged rename comes from.
pub fn local_diff(root: &str, path: &str, old: Option<&str>, staged: bool) -> Result<FileDiffView, String> {
    let path = checked(path)?;
    let old = old.map(checked).transpose()?.unwrap_or(path);
    if !staged && !tracked(root, path)? {
        // An untracked file: all of it is new.
        let data = std::fs::read(std::path::Path::new(root).join(path)).map_err(|error| error.to_string())?;
        let binary = data.iter().take(8000).any(|byte| *byte == 0);
        let text = String::from_utf8_lossy(&data);
        let mut lines: Vec<&str> = text.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let hunk = DiffHunk {
            old_start: 0,
            old_count: 0,
            new_start: 1,
            new_count: lines.len() as u32,
            section: String::new(),
            lines: lines
                .iter()
                .enumerate()
                .map(|(at, line)| DiffLine {
                    kind: LineKind::Add,
                    old: None,
                    new: Some(at as u32 + 1),
                    text: line.strip_suffix('\r').unwrap_or(line).to_owned(),
                })
                .collect(),
        };
        let hunks = if binary || lines.is_empty() { Vec::new() } else { vec![hunk] };
        return Ok(FileDiffView { path: path.into(), old_path: path.into(), binary, hunks });
    }
    let mut args = vec!["diff", "--no-color", "-U3", "-M"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    if staged && old != path {
        args.push(old);
    }
    args.push(path);
    let out = git_bytes(root, &args)?;
    let (binary, hunks) = parse_hunks(&String::from_utf8_lossy(&out));
    Ok(FileDiffView { path: path.into(), old_path: old.into(), binary, hunks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_from_the_window_stay_inside_the_folder() {
        assert!(checked("src/a.ts").is_ok());
        for bad in ["", "-a", "/etc/passwd", "../x", "a/../../x", "C:/x", "a\\b", "a//b", "./a"] {
            assert!(checked(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_hunks_with_both_sides_numbers() {
        let (binary, hunks) = parse_hunks("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,3 +1,3 @@ fn x\n a\n-b\n+c\n d\n\\ No newline at end of file\n");
        assert!(!binary);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].section, "fn x");
        let lines: Vec<String> = hunks[0]
            .lines
            .iter()
            .map(|line| format!("{:?} {:?} {:?} {}", line.kind, line.old, line.new, line.text))
            .collect();
        assert_eq!(
            lines,
            ["Context Some(1) Some(1) a", "Del Some(2) None b", "Add None Some(2) c", "Context Some(3) Some(3) d"]
        );
    }
}
