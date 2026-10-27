//! The reads behind each click, in Haunt's own process with gix: a commit with its refs and files, the files between two
//! commits, and one file's diff. gix reads objects as git stored them: no filter, textconv or diff driver of the
//! repository runs (the tree diff reads blobs straight from the object database, and line diffs are worked out here).
//! Paths are relative to the opened folder, like the scan's, when the folder is inside a larger repository.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use gix::bstr::ByteSlice;
use gix::diff::blob::{Algorithm, Diff, InternedInput};
use gix::object::tree::diff::ChangeDetached;

use super::cli::revision;
use super::types::{
    ChangedPath, CommitDetail, Decoration, DecorationKind, DiffHunk, DiffLine, FileDiffView, LineKind, PathStatus,
};

/// A repository opened once per folder, shared between threads.
fn open(root: &str) -> Result<(gix::Repository, String), String> {
    static OPEN: OnceLock<Mutex<HashMap<String, (gix::ThreadSafeRepository, String)>>> = OnceLock::new();
    let mut known = OPEN.get_or_init(Mutex::default).lock().unwrap();
    if let Some((repo, prefix)) = known.get(root) {
        return Ok((repo.to_thread_local(), prefix.clone()));
    }
    let repo = gix::discover(root).map_err(|error| {
        said!("objects.notRepository", "{root} is not in a git repository: {error}", root = root, error = error)
    })?;
    // The opened folder's place in the repository ("" at its root, "packages/app/" deeper in).
    let prefix = repo
        .workdir()
        .and_then(|workdir| std::fs::canonicalize(workdir).ok())
        .map(crate::paths::plain)
        .and_then(|workdir| Path::new(root).strip_prefix(&workdir).ok().map(Path::to_path_buf))
        .map(|inner| {
            inner
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .map(|part| format!("{part}/"))
                .collect::<String>()
        })
        .unwrap_or_default();
    known.insert(root.to_owned(), (repo.clone().into_sync(), prefix.clone()));
    Ok((repo, prefix))
}

/// Whether each commit carries a signature: a `gpgsig` header, read in the process; checking it is
/// `signature::check`'s, for one commit at a time.
pub fn signed<'a>(root: &str, shas: impl Iterator<Item = &'a str>) -> Vec<bool> {
    let Ok((repo, _)) = open(root) else { return shas.map(|_| false).collect() };
    shas.map(|sha| {
        let Ok(id) = gix::ObjectId::from_hex(sha.as_bytes()) else { return false };
        repo.find_commit(id)
            .ok()
            .and_then(|commit| {
                commit.decode().ok().map(|decoded| {
                    decoded.extra_headers.iter().any(|(name, _)| *name == "gpgsig" || *name == "gpgsig-sha256")
                })
            })
            .unwrap_or(false)
    })
    .collect()
}

fn commit_of<'repo>(repo: &'repo gix::Repository, name: &str) -> Result<gix::Commit<'repo>, String> {
    let name = revision(name)?;
    repo.rev_parse_single(name)
        .map_err(|_| said!("objects.notFound", "\"{name}\" is not a commit, branch or tag here.", name = name))?
        .object()
        .map_err(|error| error.to_string())?
        .peel_to_commit()
        .map_err(|_| said!("objects.notCommit", "\"{name}\" is not a commit.", name = name))
}

/// A blob's bytes, or None when it is not there.
fn blob(repo: &gix::Repository, id: gix::ObjectId) -> Option<Vec<u8>> {
    // Taken, not copied: a big file is in memory once.
    repo.find_blob(id).ok().map(|mut blob| blob.take_data())
}

/// Binary as git decides it: a zero byte in the first 8000.
fn binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|byte| *byte == 0)
}

/// Lines added and removed between two versions of a text.
fn line_counts(before: &[u8], after: &[u8]) -> (u32, u32) {
    let (before, after) = (before.to_str_lossy(), after.to_str_lossy());
    let input = InternedInput::new(before.as_ref(), after.as_ref());
    let diff = Diff::compute(Algorithm::Histogram, &input);
    (diff.count_additions(), diff.count_removals())
}

/// The files changed between two commits (`from` None: everything `to` has, its first commit), with their status and
/// line counts, renames found as git finds them (50% alike).
pub fn files_between(root: &str, from: Option<&str>, to: &str) -> Result<Vec<ChangedPath>, String> {
    let (repo, prefix) = open(root)?;
    let new_tree = commit_of(&repo, to)?.tree().map_err(|error| error.to_string())?;
    let old_tree = match from {
        Some(from) => Some(commit_of(&repo, from)?.tree().map_err(|error| error.to_string())?),
        None => None,
    };
    let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites::default()));
    let changes =
        repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), options).map_err(|error| error.to_string())?;
    let inside = |location: &[u8]| -> Option<String> {
        location.to_str_lossy().strip_prefix(prefix.as_str()).map(str::to_owned)
    };
    let mut out = Vec::new();
    for change in changes {
        let (status, path, old_path, before, after) = match change {
            ChangeDetached::Addition { location, entry_mode, id, .. } if !entry_mode.is_tree() => {
                (PathStatus::Added, location.clone(), location, None, Some(id))
            }
            ChangeDetached::Deletion { location, entry_mode, id, .. } if !entry_mode.is_tree() => {
                (PathStatus::Deleted, location.clone(), location, Some(id), None)
            }
            ChangeDetached::Modification { location, previous_entry_mode, previous_id, entry_mode, id }
                if !entry_mode.is_tree() =>
            {
                let status = if previous_id == id && previous_entry_mode != entry_mode {
                    PathStatus::Type
                } else {
                    PathStatus::Modified
                };
                (status, location.clone(), location, Some(previous_id), Some(id))
            }
            ChangeDetached::Rewrite { source_location, source_id, entry_mode, id, location, copy, .. }
                if !entry_mode.is_tree() =>
            {
                (
                    if copy { PathStatus::Copied } else { PathStatus::Renamed },
                    location,
                    source_location,
                    Some(source_id),
                    Some(id),
                )
            }
            _ => continue,
        };
        let (Some(path), Some(old_path)) = (inside(&path), inside(&old_path).or_else(|| inside(&path))) else {
            continue;
        };
        let before = before.and_then(|id| blob(&repo, id)).unwrap_or_default();
        let after = after.and_then(|id| blob(&repo, id)).unwrap_or_default();
        let is_binary = binary(&before) || binary(&after);
        let (added, removed) = if is_binary { (0, 0) } else { line_counts(&before, &after) };
        out.push(ChangedPath { path, old_path, status, added, removed, binary: is_binary });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The branches, remote branches and tags that point at `sha`, and HEAD when it does.
fn decorations_of(repo: &gix::Repository, sha: gix::ObjectId) -> Vec<Decoration> {
    let mut out = Vec::new();
    if repo.head_id().is_ok_and(|head| head.detach() == sha) {
        out.push(Decoration { kind: DecorationKind::Head, name: "HEAD".into() });
    }
    let Ok(platform) = repo.references() else { return out };
    let Ok(all) = platform.all() else { return out };
    // Peeled with the packed refs read once (they hold each packed tag's commit), and without decoding commits.
    let packed = repo.refs.cached_packed_buffer().ok().flatten();
    for reference in all.flatten() {
        let mut reference = reference;
        let name = reference.name().as_bstr().to_str_lossy().into_owned();
        let found = if let Some(rest) = name.strip_prefix("refs/heads/") {
            Some((DecorationKind::Branch, rest.to_owned()))
        } else if let Some(rest) = name.strip_prefix("refs/remotes/").filter(|rest| !rest.ends_with("/HEAD")) {
            Some((DecorationKind::Remote, rest.to_owned()))
        } else {
            name.strip_prefix("refs/tags/").map(|rest| (DecorationKind::Tag, rest.to_owned()))
        };
        let Some((kind, short)) = found else { continue };
        if reference.peel_to_id_packed(packed.as_deref().map(|snapshot| &**snapshot)).is_ok_and(|id| id.detach() == sha)
        {
            out.push(Decoration { kind, name: short });
        }
    }
    out
}

/// A commit's message, refs and files (against its first parent; a merge's other parents are not compared).
pub fn commit(root: &str, sha: &str) -> Result<CommitDetail, String> {
    let (repo, _) = open(root)?;
    let found = commit_of(&repo, sha)?;
    let id = found.id;
    let parents: Vec<String> = found.parent_ids().map(|parent| parent.to_string()).collect();
    let author = found.author().map_err(|error| error.to_string())?;
    let committer = found.committer().map_err(|error| error.to_string())?;
    let message = found.message_raw().map_err(|error| error.to_string())?.to_str_lossy().into_owned();
    let (subject, body) = message
        .split_once('\n')
        .map(|(subject, body)| (subject.trim().to_owned(), body.trim().to_owned()))
        .unwrap_or_else(|| (message.trim().to_owned(), String::new()));
    let seconds = |signature: &gix::actor::SignatureRef<'_>| signature.time().map(|time| time.seconds).unwrap_or(0);
    let detail = CommitDetail {
        sha: id.to_string(),
        author: author.name.to_str_lossy().into_owned(),
        email: author.email.to_str_lossy().into_owned(),
        time: seconds(&author),
        committer: committer.name.to_str_lossy().into_owned(),
        committer_time: seconds(&committer),
        subject,
        body,
        refs: decorations_of(&repo, id),
        files: files_between(root, parents.first().map(String::as_str), &id.to_string())?,
        parents,
    };
    Ok(detail)
}

/// The text of `path` (relative to the opened folder) at a commit, or None when it did not exist there or is not text.
/// Read from the object database: no filter or textconv runs.
pub fn file_at(root: &str, sha: &str, path: &str) -> Option<String> {
    let (repo, prefix) = open(root).ok()?;
    let data = text_at(&repo, &prefix, sha, path).ok()??;
    String::from_utf8(data).ok()
}

/// The text of `path` (relative to the opened folder) at a commit, or None when it did not exist there.
fn text_at(repo: &gix::Repository, prefix: &str, sha: &str, path: &str) -> Result<Option<Vec<u8>>, String> {
    let tree = commit_of(repo, sha)?.tree().map_err(|error| error.to_string())?;
    let full = format!("{prefix}{path}");
    let entry = tree.lookup_entry_by_path(PathBuf::from(&full)).map_err(|error| error.to_string())?;
    Ok(entry.and_then(|entry| blob(repo, entry.object_id())))
}

/// The function a hunk is in, as git's default finds it: the nearest line above that starts with a letter, `$` or `_`.
fn section(lines: &[&str], before: usize) -> String {
    lines[..before.min(lines.len())]
        .iter()
        .rev()
        .find(|line| line.chars().next().is_some_and(|c| c.is_alphabetic() || c == '$' || c == '_'))
        .map(|line| line.chars().take(80).collect::<String>().trim_end().to_owned())
        .unwrap_or_default()
}

/// The hunks between two texts, with `context` lines around each change; changes closer than twice that share a hunk.
#[cfg(test)]
pub fn hunks(before: &str, after: &str, context: u32) -> Vec<DiffHunk> {
    hunks_with(before, after, context, false)
}

/// A text with the whitespace taken out of every line, the lines themselves kept: compared instead of the text when
/// whitespace is ignored, so the hunks still name and show the real lines.
fn without_space(text: &str) -> String {
    let mut out: String =
        text.lines().map(|line| line.split_whitespace().collect::<String>()).collect::<Vec<_>>().join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// As `hunks`; `ignore_space` leaves out changes of whitespace only (`git diff -w`).
pub fn hunks_with(before: &str, after: &str, context: u32, ignore_space: bool) -> Vec<DiffHunk> {
    let (compared_before, compared_after) = if ignore_space {
        (without_space(before), without_space(after))
    } else {
        (before.to_owned(), after.to_owned())
    };
    let input = InternedInput::new(compared_before.as_str(), compared_after.as_str());
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let changes: Vec<_> = diff.hunks().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < changes.len() {
        // A group: changes whose context windows touch.
        let mut end = at;
        while end + 1 < changes.len()
            && changes[end + 1].before.start.saturating_sub(changes[end].before.end) <= 2 * context
        {
            end += 1;
        }
        let first = &changes[at];
        let last = &changes[end];
        let old_start = first.before.start.saturating_sub(context);
        let new_start = first.after.start - (first.before.start - old_start);
        let old_end = (last.before.end + context).min(old.len() as u32);
        let mut lines = Vec::new();
        let (mut old_at, mut new_at) = (old_start, new_start);
        for change in &changes[at..=end] {
            while old_at < change.before.start {
                lines.push(DiffLine {
                    kind: LineKind::Context,
                    old: Some(old_at + 1),
                    new: Some(new_at + 1),
                    text: old[old_at as usize].to_owned(),
                });
                old_at += 1;
                new_at += 1;
            }
            for line in change.before.clone() {
                lines.push(DiffLine {
                    kind: LineKind::Del,
                    old: Some(line + 1),
                    new: None,
                    text: old.get(line as usize).copied().unwrap_or_default().to_owned(),
                });
            }
            for line in change.after.clone() {
                lines.push(DiffLine {
                    kind: LineKind::Add,
                    old: None,
                    new: Some(line + 1),
                    text: new.get(line as usize).copied().unwrap_or_default().to_owned(),
                });
            }
            old_at = change.before.end;
            new_at = change.after.end;
        }
        while old_at < old_end {
            lines.push(DiffLine {
                kind: LineKind::Context,
                old: Some(old_at + 1),
                new: Some(new_at + 1),
                text: old[old_at as usize].to_owned(),
            });
            old_at += 1;
            new_at += 1;
        }
        let old_count = lines.iter().filter(|line| line.old.is_some()).count() as u32;
        let new_count = lines.iter().filter(|line| line.new.is_some()).count() as u32;
        // As git writes it: a side with no lines starts at the line before (0 for an empty file).
        out.push(DiffHunk {
            old_start: if old_count == 0 { old_start } else { old_start + 1 },
            old_count,
            new_start: if new_count == 0 { new_start } else { new_start + 1 },
            new_count,
            section: section(&old, old_start as usize),
            lines,
        });
        at = end + 1;
    }
    out
}

/// One file's diff between two commits (`from` None: its first commit), following a rename from `old_path`.
#[cfg(test)]
pub fn file_diff(
    root: &str,
    from: Option<&str>,
    to: &str,
    path: &str,
    old_path: &str,
    context: u32,
) -> Result<FileDiffView, String> {
    file_diff_with(root, from, to, path, old_path, context, false)
}

/// As `file_diff`; `ignore_space` leaves out changes of whitespace only.
pub fn file_diff_with(
    root: &str,
    from: Option<&str>,
    to: &str,
    path: &str,
    old_path: &str,
    context: u32,
    ignore_space: bool,
) -> Result<FileDiffView, String> {
    let (repo, prefix) = open(root)?;
    let after = text_at(&repo, &prefix, to, path)?.unwrap_or_default();
    let before = match from {
        Some(from) => text_at(&repo, &prefix, from, old_path)?.unwrap_or_default(),
        None => Vec::new(),
    };
    let view = |hunks, is_binary| FileDiffView {
        path: path.to_owned(),
        old_path: old_path.to_owned(),
        binary: is_binary,
        hunks,
    };
    if binary(&before) || binary(&after) {
        return Ok(view(Vec::new(), true));
    }
    Ok(view(hunks_with(&before.to_str_lossy(), &after.to_str_lossy(), context.min(1_000_000), ignore_space), false))
}
