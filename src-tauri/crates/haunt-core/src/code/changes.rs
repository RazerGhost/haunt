//! What a change does function by function: the working tree against a base, or one commit against another. The
//! changed lines of every file (`git/diff.rs`) fall in the innermost function that holds them, in the version they
//! belong to: added lines in the newer version, removed lines in the older one. A function counts as changed when lines
//! inside it changed, not when only a function written within it did. Functions are matched across the two versions by
//! their names from the top of the file down (`Order › total`), so moving one down a file is no change, and one that is
//! only in one version was added or removed.
//!
//! The versions' functions come from the scan where it has them (the working tree) and are read otherwise: the
//! TypeScript family by the sidecar, PHP, Rust and Lua here (`ReadVersion`).
//!
//! A function removed in one place and added in another with the same body, or nearly (another file, another name, or
//! into a class), is one function moved: it is listed once, where it is now, as `moved`, with where it came from
//! (`from`), and the lines added and removed are those that differ between the two bodies.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::languages::language_of;
use super::types::{FunctionKind, FunctionNode};
use crate::git::diff::{self, Base, FileDiff, FileStatus, Head, Range};

/// A file of the last scan, as the changes need it.
#[derive(Debug, Clone, Deserialize)]
pub struct ScannedFile {
    pub path: String,
    pub part: String,
    pub functions: Vec<FunctionNode>,
}

/// The functions of a version of a file: its path and text in, its functions out.
pub type ReadVersion<'a> = dyn Fn(&str, &str) -> Vec<FunctionNode> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum FunctionStatus {
    Added,
    Changed,
    Removed,
    /// Removed in one place and added in another with the same body, or nearly: `from` says where it was.
    Moved,
}

/// Where a moved function was before.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MovedFrom {
    pub path: String,
    pub name: String,
    pub qualified: String,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFunction {
    /// The function's id in the current scan; null when it was removed, or the working tree no longer has it.
    pub id: Option<String>,
    pub name: String,
    /// Its names from the top of the file down (`Order › total`), which match it across the two versions.
    pub qualified: String,
    pub kind: FunctionKind,
    pub status: FunctionStatus,
    /// Lines added and removed inside it (not inside the functions written within it).
    pub added: u32,
    pub removed: u32,
    /// Where it is now, or where it was when removed.
    pub line: u32,
    pub end_line: u32,
    /// For a moved function, where it was.
    pub from: Option<MovedFrom>,
    /// Its lines in the older version (a changed or removed function), for who wrote what the change touches.
    pub before: Option<LineSpan>,
}

/// Lines of a version of a file.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineSpan {
    pub line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub old_path: String,
    pub status: FileStatus,
    /// The part it is in now; null for a file the scan does not have.
    pub part: Option<String>,
    pub added: u32,
    pub removed: u32,
    /// Whether Haunt reads the file (a source file of a language it knows).
    pub code: bool,
    pub functions: Vec<ChangedFunction>,
    /// A version of it could not be read (the reader failed or took too long): its functions are left out rather
    /// than shown wrong.
    pub unread: bool,
}

/// What the working tree changes compared with a base.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Changes {
    pub head: Head,
    pub base: Base,
    pub files: Vec<ChangedFile>,
}

/// What a commit or a range of commits changes.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RangeChanges {
    /// The older commit; null when `to` is the first commit.
    pub from: Option<String>,
    pub to: String,
    pub files: Vec<ChangedFile>,
}

struct Flat<'a> {
    node: &'a FunctionNode,
    qualified: String,
    size: u32,
}

fn flatten(tree: &[FunctionNode]) -> Vec<Flat<'_>> {
    fn walk<'a>(items: &'a [FunctionNode], prefix: &str, seen: &mut HashMap<String, u32>, out: &mut Vec<Flat<'a>>) {
        for node in items {
            let mut qualified =
                if prefix.is_empty() { node.name.clone() } else { format!("{prefix} › {}", node.name) };
            // Two functions with the same names in one file (two anonymous callbacks, an overload): number them in order.
            let count = seen.entry(qualified.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                qualified = format!("{qualified} #{count}");
            }
            out.push(Flat { node, qualified: qualified.clone(), size: node.end_line - node.line });
            walk(&node.children, &qualified, seen, out);
        }
    }
    let mut out = Vec::new();
    walk(tree, "", &mut HashMap::new(), &mut out);
    out
}

/// For each line in the ranges, the innermost function that holds it, counted by qualified name.
fn count_lines(functions: &[Flat], ranges: &[Range], line_count: u32) -> HashMap<String, u32> {
    let mut by_size: Vec<&Flat> = functions.iter().collect();
    by_size.sort_by_key(|item| item.size);
    let mut counts = HashMap::new();
    for range in ranges {
        let end = range.start.saturating_add(range.count).saturating_sub(1).min(line_count);
        for line in range.start..=end {
            if let Some(owner) = by_size.iter().find(|item| item.node.line <= line && line <= item.node.end_line) {
                *counts.entry(owner.qualified.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Whether any line of the function is in the ranges.
fn touched(node: &FunctionNode, ranges: &[Range]) -> bool {
    ranges.iter().any(|range| {
        range.count > 0 && range.start <= node.end_line && node.line < range.start.saturating_add(range.count)
    })
}

fn total(ranges: &[Range], cap: u32) -> u32 {
    ranges.iter().map(|range| range.count.min(cap)).fold(0, u32::saturating_add)
}

fn line_count(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// A version of a file: its functions, how many lines it has, and its text.
struct Version {
    functions: Vec<FunctionNode>,
    lines: u32,
    text: String,
}

/// The bodies of a file's added and removed functions, by qualified name, for finding the moved ones.
type Bodies = HashMap<String, Vec<String>>;

/// Lines `start` to `end` of `text`.
fn body(text: &str, start: u32, end: u32) -> Vec<String> {
    text.split('\n')
        .skip(start.saturating_sub(1) as usize)
        .take((end + 1).saturating_sub(start) as usize)
        .map(str::to_owned)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn file_changes(
    root: &str,
    older: Option<&str>,
    diff: &FileDiff,
    part: Option<String>,
    code: bool,
    now: Option<Version>,
    read: &ReadVersion,
    id_of: &dyn Fn(&FunctionNode, &str) -> Option<String>,
) -> (ChangedFile, Bodies) {
    let mut bodies = Bodies::new();
    let now_lines = now.as_ref().map_or(0, |now| now.lines);
    let now_text = now.as_ref().map(|now| now.text.clone()).unwrap_or_default();
    let mut file = ChangedFile {
        path: diff.path.clone(),
        old_path: diff.old_path.clone(),
        status: diff.status,
        part,
        added: total(&diff.added, now_lines),
        removed: total(&diff.removed, u32::MAX),
        code,
        functions: Vec::new(),
        unread: false,
    };
    if !code {
        return (file, bodies);
    }
    let now_functions = now.map(|now| now.functions).unwrap_or_default();
    let mut then_functions = Vec::new();
    let mut then_lines = 0;
    let mut then_text = String::new();
    if let Some(older) = older.filter(|_| diff.status != FileStatus::Added && language_of(&diff.old_path).is_some()) {
        if let Some(text) = crate::git::file_at(root, older, &diff.old_path) {
            then_lines = line_count(&text);
            then_functions = read(&diff.old_path, &text);
            then_text = text;
        }
    }
    let now = flatten(&now_functions);
    let then = flatten(&then_functions);
    let added = count_lines(&now, &diff.added, now_lines);
    let removed = count_lines(&then, &diff.removed, then_lines);
    let now_by_name: HashMap<&str, &Flat> = now.iter().map(|item| (item.qualified.as_str(), item)).collect();
    let then_by_name: HashMap<&str, &Flat> = then.iter().map(|item| (item.qualified.as_str(), item)).collect();
    let mut names: Vec<&str> = now.iter().chain(then.iter()).map(|item| item.qualified.as_str()).collect();
    let mut seen = HashSet::new();
    names.retain(|name| seen.insert(*name));
    for name in names {
        let current = now_by_name.get(name);
        let before = then_by_name.get(name);
        let plus = added.get(name).copied().unwrap_or(0);
        let minus = removed.get(name).copied().unwrap_or(0);
        let status = match (current, before) {
            (Some(_), None) => FunctionStatus::Added,
            (None, Some(_)) => FunctionStatus::Removed,
            _ if plus > 0 || minus > 0 => FunctionStatus::Changed,
            _ => continue,
        };
        // Only in one version by name, yet none of its lines changed: it was renumbered (an anonymous callback after a
        // new one, `describe() #4` now `#5`), not added or removed.
        if status == FunctionStatus::Added && !touched(current.unwrap().node, &diff.added) {
            continue;
        }
        if status == FunctionStatus::Removed && !touched(before.unwrap().node, &diff.removed) {
            continue;
        }
        let node = current.or(before).unwrap().node;
        match status {
            FunctionStatus::Added => bodies.insert(name.to_owned(), body(&now_text, node.line, node.end_line)),
            FunctionStatus::Removed => bodies.insert(name.to_owned(), body(&then_text, node.line, node.end_line)),
            _ => None,
        };
        file.functions.push(ChangedFunction {
            id: current.and_then(|current| id_of(current.node, name)),
            name: node.name.clone(),
            qualified: name.to_owned(),
            kind: node.kind,
            status,
            added: plus,
            removed: minus,
            line: node.line,
            end_line: node.end_line,
            from: None,
            before: before.map(|before| LineSpan { line: before.node.line, end_line: before.node.end_line }),
        });
    }
    file.functions.sort_by_key(|function| function.line);
    (file, bodies)
}

/// A body as compared for moves: its lines trimmed, without the empty ones, with the function's own name as `\0` (so a
/// renamed function, or one that calls itself, still matches).
fn normalised(lines: &[String], name: &str) -> Vec<String> {
    lines.iter().map(|line| whole_word(line.trim(), name)).filter(|line| !line.is_empty()).collect()
}

fn whole_word(line: &str, name: &str) -> String {
    if name.is_empty() || name.starts_with('(') {
        return line.to_owned();
    }
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(word) || after.is_some_and(word) {
            out.push_str(name);
        } else {
            out.push('\0');
        }
        rest = &rest[at + name.len()..];
    }
    out.push_str(rest);
    out
}

/// How alike two bodies are, from 0 to 1 (twice the lines they share over all their lines), and how many lines each
/// has that the other lacks.
fn likeness(a: &[String], b: &[String]) -> (f64, u32, u32) {
    let mut counts: HashMap<&str, i32> = HashMap::new();
    for line in b {
        *counts.entry(line).or_insert(0) += 1;
    }
    let mut shared = 0;
    for line in a {
        if let Some(count) = counts.get_mut(line.as_str()).filter(|count| **count > 0) {
            *count -= 1;
            shared += 1;
        }
    }
    let total = a.len() + b.len();
    let score = if total == 0 { 0.0 } else { 2.0 * shared as f64 / total as f64 };
    (score, (a.len() - shared) as u32, (b.len() - shared) as u32)
}

/// Whether two bodies are alike enough to be one function moved: most of their lines the same in a body of three lines
/// or more, or the very same text in a shorter one that is not trivially short.
fn same_function(a: &[String], b: &[String]) -> Option<(f64, u32, u32)> {
    let (score, added, removed) = likeness(a, b);
    let long = a.len().min(b.len()) >= 3 && score >= 0.75;
    let short = a == b && a.iter().map(|line| line.chars().filter(|c| !c.is_whitespace()).count()).sum::<usize>() >= 40;
    (long || short).then_some((score, added, removed))
}

/// Pairs the removed functions with the added ones that have their body, across every file, best match first; each
/// pair becomes one moved function where the added one is.
fn pair_moves(files: &mut [ChangedFile], bodies: &[Bodies]) {
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for (at, file) in files.iter().enumerate() {
        for (index, function) in file.functions.iter().enumerate() {
            let Some(lines) = bodies[at].get(&function.qualified) else { continue };
            let lines = normalised(lines, &function.name);
            match function.status {
                FunctionStatus::Removed => removed.push((at, index, lines)),
                FunctionStatus::Added => added.push((at, index, lines)),
                _ => {}
            }
        }
    }
    if removed.is_empty() || added.is_empty() {
        return;
    }
    let mut pairs = Vec::new();
    for (a, (_, _, new)) in added.iter().enumerate() {
        for (r, (_, _, old)) in removed.iter().enumerate() {
            if let Some((score, plus, minus)) = same_function(new, old) {
                pairs.push((score, a, r, plus, minus));
            }
        }
    }
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
    let (mut used_added, mut used_removed) = (HashSet::new(), HashSet::new());
    let mut drop: Vec<(usize, usize)> = Vec::new();
    for (_, a, r, plus, minus) in pairs {
        if used_added.contains(&a) || used_removed.contains(&r) {
            continue;
        }
        used_added.insert(a);
        used_removed.insert(r);
        let (to_file, to_index, _) = &added[a];
        let (from_file, from_index, _) = &removed[r];
        let old = files[*from_file].functions[*from_index].clone();
        let path = files[*from_file].old_path.clone();
        let function = &mut files[*to_file].functions[*to_index];
        function.status = FunctionStatus::Moved;
        function.added = plus;
        function.removed = minus;
        function.from =
            Some(MovedFrom { path, name: old.name, qualified: old.qualified, line: old.line, end_line: old.end_line });
        drop.push((*from_file, *from_index));
    }
    drop.sort_by(|x, y| y.cmp(x));
    for (file, index) in drop {
        files[file].functions.remove(index);
    }
}

/// The most moves a function's history follows, and the most files a commit may change for Haunt to look for a move
/// in it (each version of each file is read).
const MOVES_FOLLOWED: usize = 3;
const MOVE_COMMIT_FILES: usize = 60;

/// Carries a function's history on past the commits that moved it here from another file: `git log -L` follows lines
/// within a file (a rename inside it too) and across a file's renames, and stops where the lines first appear. When
/// that commit moved the function (`pair_moves`), the history goes on from where it was, at the commit before.
pub fn follow_moves(
    root: &str,
    history: &mut crate::git::types::FunctionHistory,
    path: &str,
    start: u32,
    files: &HashMap<String, ScannedFile>,
    read: &ReadVersion,
) {
    let Some(mut name) = files.get(path).and_then(|file| {
        flatten(&file.functions).into_iter().find(|item| item.node.line == start).map(|item| item.node.name.clone())
    }) else {
        return;
    };
    let mut path = path.to_owned();
    let mut part = history.commits.clone();
    for _ in 0..MOVES_FOLLOWED {
        if part.is_empty() || part.len() >= crate::git::HISTORY_COMMITS {
            return;
        }
        let oldest = part.last().unwrap().sha.clone();
        let Some((parent, size)) = crate::git::parent_and_size(root, &oldest) else { return };
        if size > MOVE_COMMIT_FILES {
            return;
        }
        let Ok(changes) = changes_between(root, Some(&parent), &oldest, files, read) else { return };
        let moved: Vec<(&ChangedFile, &ChangedFunction)> = changes
            .files
            .iter()
            .flat_map(|file| file.functions.iter().map(move |function| (file, function)))
            .filter(|(_, function)| function.status == FunctionStatus::Moved)
            .collect();
        // The moved function by its name; else the only one moved into this file.
        let found = moved.iter().find(|(_, function)| function.name == name).or_else(|| {
            let here: Vec<_> = moved.iter().filter(|(file, _)| file.path == path).collect();
            (here.len() == 1).then(|| here[0])
        });
        let Some((file, function)) = found else { return };
        let from = function.from.clone().unwrap();
        // Moved within its file is a change `git log -L` already followed.
        if from.path == file.path {
            return;
        }
        let Ok(older) = crate::git::line_history(root, &from.path, from.line, from.end_line, Some(&parent)) else {
            return;
        };
        history.moves.push(crate::git::types::HistoryMove {
            sha: oldest,
            from_path: from.path.clone(),
            from_name: from.name.clone(),
            to_path: file.path.clone(),
            to_name: function.name.clone(),
        });
        let known: HashSet<String> = history.commits.iter().map(|commit| commit.sha.clone()).collect();
        history.complete = older.len() < crate::git::HISTORY_COMMITS;
        history.cosmetic.extend(older.iter().filter(|line| line.cosmetic).map(|line| line.commit.sha.clone()));
        let older: Vec<crate::git::types::CommitInfo> = older.into_iter().map(|line| line.commit).collect();
        history.commits.extend(older.iter().filter(|commit| !known.contains(&commit.sha)).cloned());
        part = older;
        path = from.path;
        name = from.name;
    }
}

/// What the working tree changes compared with `base`, function by function, on the functions of the last scan.
pub fn changes_of(
    root: &str,
    wanted: diff::Wanted,
    files: &HashMap<String, ScannedFile>,
    read: &ReadVersion,
) -> Result<Changes, String> {
    let head = diff::head(root);
    let base = diff::resolve_base(root, wanted, &head)?;
    let mut changed = Vec::new();
    let mut bodies = Vec::new();
    for diff in diff::working_tree_diff(root, &base.sha)? {
        let scanned = (diff.status != FileStatus::Deleted).then(|| files.get(&diff.path)).flatten();
        // The working tree's side: the scan's functions and ids, and the file as it is on disk.
        let now = scanned.map(|scanned| {
            let text = std::fs::read_to_string(Path::new(root).join(&diff.path)).unwrap_or_default();
            Version {
                functions: scanned.functions.clone(),
                lines: if text.is_empty() { 0 } else { line_count(&text) },
                text,
            }
        });
        let code = !diff.binary
            && (scanned.is_some() || (diff.status == FileStatus::Deleted && language_of(&diff.old_path).is_some()));
        let part = scanned.map(|scanned| scanned.part.clone());
        let now = if code && diff.status != FileStatus::Deleted { now } else { None };
        let (file, found) =
            file_changes(root, Some(&base.sha), &diff, part, code, now, read, &|node, _| Some(node.id.clone()));
        changed.push(file);
        bodies.push(found);
    }
    pair_moves(&mut changed, &bodies);
    changed.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Changes { head, base, files: changed })
}

/// What `to` changes compared with `from` (None: everything `to` has, its first commit), function by function. Both
/// versions are read from git. A function the working tree still has under the same name carries its id in the scan,
/// so the lenses can find its callers in the code as it is now.
pub fn changes_between(
    root: &str,
    from: Option<&str>,
    to: &str,
    files: &HashMap<String, ScannedFile>,
    read: &ReadVersion,
) -> Result<RangeChanges, String> {
    let diffs = diff::commit_diff(root, from, to)?;
    let changed = stored_changes(root, from, diffs, files, read, |path| crate::git::file_at(root, to, path));
    Ok(RangeChanges { from: from.map(str::to_owned), to: to.to_owned(), files: changed })
}

/// What the staged changes change compared with the last commit, function by function: what a commit made now would
/// change. The staged versions are read from the index; a function the working tree still has under the same name
/// carries its id in the scan.
pub fn staged_changes(
    root: &str,
    files: &HashMap<String, ScannedFile>,
    read: &ReadVersion,
) -> Result<RangeChanges, String> {
    let head = diff::head(root).sha;
    let diffs = diff::staged_diff(root)?;
    let changed = stored_changes(root, head.as_deref(), diffs, files, read, |path| diff::index_text(root, path));
    Ok(RangeChanges { from: head, to: "index".into(), files: changed })
}

/// The files `diffs` changes, function by function, when both versions are stored in git: the older at `from`, the newer
/// read by `newer` (a commit's or the index's). Functions are matched to the scan's ids by name.
fn stored_changes(
    root: &str,
    from: Option<&str>,
    diffs: Vec<FileDiff>,
    files: &HashMap<String, ScannedFile>,
    read: &ReadVersion,
    newer: impl Fn(&str) -> Option<String>,
) -> Vec<ChangedFile> {
    let mut changed = Vec::new();
    let mut bodies = Vec::new();
    for diff in diffs {
        let scanned = files.get(&diff.path);
        let ids: HashMap<String, String> = scanned
            .map(|scanned| {
                flatten(&scanned.functions).into_iter().map(|item| (item.qualified, item.node.id.clone())).collect()
            })
            .unwrap_or_default();
        let source = if diff.status == FileStatus::Deleted { &diff.old_path } else { &diff.path };
        let code = !diff.binary && language_of(source).is_some();
        let now = if code && diff.status != FileStatus::Deleted {
            newer(&diff.path).map(|text| Version { functions: read(&diff.path, &text), lines: line_count(&text), text })
        } else {
            None
        };
        let part = scanned.map(|scanned| scanned.part.clone());
        let (file, found) =
            file_changes(root, from, &diff, part, code, now, read, &|_, qualified| ids.get(qualified).cloned());
        changed.push(file);
        bodies.push(found);
    }
    pair_moves(&mut changed, &bodies);
    changed.sort_by(|a, b| a.path.cmp(&b.path));
    changed
}
