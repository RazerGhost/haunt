//! A patch of selected lines, from one file's `git diff`: what staging, unstaging or discarding a few lines applies.
//!
//! Forward (staging lines of the unstaged diff, applied to the index): a selected line keeps its change; an unselected
//! removed line stays, as context, and an unselected added line is left out. Reverse (unstaging lines of the staged diff,
//! or discarding lines of the unstaged one; applied with `--reverse`): the same with the sides swapped, since the newer
//! side is what the patch is applied to. Hunk headers are counted again, so `git apply` takes the patch as it is.

use serde::Deserialize;
use specta::Type;

/// A changed line, by its number: a removed line by its number on the older side, an added one on the newer side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
pub struct LineRef {
    pub old: Option<u32>,
    pub new: Option<u32>,
}

/// A removed or added line of a run of changes.
struct Change<'a> {
    picked: bool,
    content: &'a [u8],
    /// Git's "\ No newline at end of file" after it.
    note: Option<&'a [u8]>,
}

/// What the line before a no-newline note was.
#[derive(Clone, Copy)]
enum Last {
    Context,
    Removed,
    Added,
}

fn range(text: &str) -> (u32, u32) {
    let text = text.trim_start_matches(['-', '+']);
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    (start.parse().unwrap_or(0), count.parse().unwrap_or(1))
}

/// The patch of the selected lines, or None when none of them is a change in `diff`. Refused for a file that is added,
/// deleted or binary: those are staged, unstaged or discarded whole.
pub fn select(diff: &[u8], selected: &[LineRef], reverse: bool) -> Result<Option<Vec<u8>>, String> {
    let lines: Vec<&[u8]> = diff.split(|byte| *byte == b'\n').collect();
    let first_hunk = lines.iter().position(|line| line.starts_with(b"@@ ")).unwrap_or(lines.len());
    let header = &lines[..first_hunk];
    let whole = header.iter().any(|line| {
        line.starts_with(b"new file mode")
            || line.starts_with(b"deleted file mode")
            || line.starts_with(b"Binary files")
    });
    if whole {
        return Err(said!(
            "patch.wholeFile",
            "Lines of a new, deleted or binary file cannot be picked: take the whole file."
        ));
    }
    let mut out: Vec<u8> = Vec::new();
    for line in header {
        if !line.is_empty() {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
    // How far the side the patch is built against has moved, from the hunks before.
    let mut delta: i64 = 0;
    let mut any = false;
    let mut at = first_hunk;
    while at < lines.len() {
        let head = String::from_utf8_lossy(lines[at]).into_owned();
        let Some(rest) = head.strip_prefix("@@ ") else {
            at += 1;
            continue;
        };
        let mut sides = rest.split(' ');
        let (old_start, _) = range(sides.next().unwrap_or_default());
        let (new_start, _) = range(sides.next().unwrap_or_default());
        let (mut old, mut new) = (old_start, new_start);
        // Each line as its mark (' ', '-', '+' or the backslash of git's no-newline note) and its content.
        let mut body: Vec<(u8, &[u8])> = Vec::new();
        let (mut old_count, mut new_count, mut changes) = (0u32, 0u32, 0u32);
        // The run of changes being read: its removed and added lines, each picked or not, with its no-newline note.
        let mut removed: Vec<Change> = Vec::new();
        let mut added: Vec<Change> = Vec::new();
        let mut last = Last::Context;
        at += 1;
        loop {
            let line = lines.get(at).copied().filter(|line| !line.starts_with(b"@@ "));
            match line.and_then(|line| line.first().copied()) {
                Some(b'-') => {
                    let picked = selected.iter().any(|pick| pick.old == Some(old) && pick.new.is_none());
                    removed.push(Change { picked, content: &line.unwrap()[1..], note: None });
                    old += 1;
                    at += 1;
                    last = Last::Removed;
                    continue;
                }
                Some(b'+') => {
                    let picked = selected.iter().any(|pick| pick.new == Some(new) && pick.old.is_none());
                    added.push(Change { picked, content: &line.unwrap()[1..], note: None });
                    new += 1;
                    at += 1;
                    last = Last::Added;
                    continue;
                }
                Some(b'\\') => {
                    // "\ No newline at end of file" belongs to the line before it.
                    let note = &line.unwrap()[1..];
                    match last {
                        Last::Removed => removed.last_mut().map(|change| change.note = Some(note)),
                        Last::Added => added.last_mut().map(|change| change.note = Some(note)),
                        Last::Context => {
                            body.push((b'\\', note));
                            None
                        }
                    };
                    at += 1;
                    continue;
                }
                _ => {}
            }
            // The run ends: removed and added lines go in pairs by position, so a kept line stays where it was
            // (staging "b → B" but not "c → C" gives a, B, c, not a, c, B).
            for pair in 0..removed.len().max(added.len()) {
                if let Some(change) = removed.get(pair) {
                    if change.picked {
                        body.push((b'-', change.content));
                        old_count += 1;
                        changes += 1;
                    } else if !reverse {
                        // Not staged: the line stays in the index, as context.
                        body.push((b' ', change.content));
                        old_count += 1;
                        new_count += 1;
                    }
                    if let Some(note) = change.note.filter(|_| change.picked || !reverse) {
                        body.push((b'\\', note));
                    }
                }
                if let Some(change) = added.get(pair) {
                    if change.picked {
                        body.push((b'+', change.content));
                        new_count += 1;
                        changes += 1;
                    } else if reverse {
                        // Not taken back: the line stays on the newer side, as context.
                        body.push((b' ', change.content));
                        old_count += 1;
                        new_count += 1;
                    }
                    if let Some(note) = change.note.filter(|_| change.picked || reverse) {
                        body.push((b'\\', note));
                    }
                }
            }
            removed.clear();
            added.clear();
            match line {
                Some(line) if line.first() == Some(&b' ') => {
                    body.push((b' ', &line[1..]));
                    old_count += 1;
                    new_count += 1;
                    old += 1;
                    new += 1;
                    last = Last::Context;
                    at += 1;
                }
                Some(_) => at += 1,
                None => break,
            }
        }
        if changes == 0 {
            continue;
        }
        any = true;
        // The side the patch is applied against keeps its numbers; the other follows from the hunks before.
        let edge = |count: u32| if count == 0 { 1 } else { 0 };
        let (from, to) = if reverse {
            let from = (new_start as i64 + delta + edge(new_count) - edge(old_count)).max(0);
            (from as u32, new_start)
        } else {
            let to = (old_start as i64 + delta + edge(old_count) - edge(new_count)).max(0);
            (old_start, to as u32)
        };
        delta += if reverse { old_count as i64 - new_count as i64 } else { new_count as i64 - old_count as i64 };
        out.extend_from_slice(format!("@@ -{from},{old_count} +{to},{new_count} @@\n").as_bytes());
        for (mark, content) in body {
            out.push(mark);
            out.extend_from_slice(content);
            out.push(b'\n');
        }
    }
    Ok(any.then_some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/f.txt b/f.txt\nindex 1..2 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -1,4 +1,4 @@\n a\n-b\n-c\n+B\n+C\n d\n@@ -10,2 +10,3 @@\n j\n+new\n k\n";

    fn text(patch: Option<Vec<u8>>) -> String {
        String::from_utf8(patch.expect("a patch")).unwrap()
    }

    #[test]
    fn staging_one_change_keeps_the_rest_out() {
        // Stage the replacement of b by B only: c stays (as context), C is left out.
        let picked = [LineRef { old: Some(2), new: None }, LineRef { old: None, new: Some(2) }];
        let patch = text(select(DIFF.as_bytes(), &picked, false).unwrap());
        assert!(patch.starts_with("diff --git a/f.txt b/f.txt\nindex 1..2 100644\n--- a/f.txt\n+++ b/f.txt\n"));
        assert!(patch.ends_with("@@ -1,4 +1,4 @@\n a\n-b\n+B\n c\n d\n"), "{patch}");
    }

    #[test]
    fn a_later_hunk_follows_the_earlier_ones() {
        // Stage only the second hunk's added line, then both hunks' additions with a line removed from the first.
        let second = text(select(DIFF.as_bytes(), &[LineRef { old: None, new: Some(11) }], false).unwrap());
        assert!(second.ends_with("@@ -10,2 +10,3 @@\n j\n+new\n k\n"), "{second}");
        let picked = [LineRef { old: Some(2), new: None }, LineRef { old: None, new: Some(11) }];
        let both = text(select(DIFF.as_bytes(), &picked, false).unwrap());
        assert!(both.contains("@@ -1,4 +1,3 @@\n a\n-b\n c\n d\n"), "{both}");
        assert!(both.ends_with("@@ -10,2 +9,3 @@\n j\n+new\n k\n"), "{both}");
    }

    #[test]
    fn reverse_takes_back_only_the_picked_lines() {
        // Take back the added C only: B stays (as context), b and c are not brought back.
        let patch = text(select(DIFF.as_bytes(), &[LineRef { old: None, new: Some(3) }], true).unwrap());
        assert!(patch.ends_with("@@ -1,3 +1,4 @@\n a\n B\n+C\n d\n"), "{patch}");
    }

    #[test]
    fn nothing_picked_is_no_patch_and_whole_files_are_refused() {
        assert_eq!(select(DIFF.as_bytes(), &[LineRef { old: Some(1), new: Some(1) }], false).unwrap(), None);
        let added = "diff --git a/n b/n\nnew file mode 100644\n--- /dev/null\n+++ b/n\n@@ -0,0 +1 @@\n+x\n";
        assert!(select(added.as_bytes(), &[LineRef { old: None, new: Some(1) }], false).is_err());
    }

    #[test]
    fn a_line_made_context_keeps_its_own_leading_marks() {
        // Lines whose text starts with a dash or a plus stay context when they are not picked.
        let diff = "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n--x\n-+y\n++z\n+-w\n";
        let patch = text(select(diff.as_bytes(), &[LineRef { old: None, new: Some(1) }], false).unwrap());
        assert!(patch.ends_with("@@ -1,2 +1,3 @@\n -x\n++z\n +y\n"), "{patch}");
    }

    #[test]
    fn keeps_carriage_returns_and_the_missing_newline_marker() {
        let diff = "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\r\n-b\r\n\\ No newline at end of file\n+c\r\n\\ No newline at end of file\n";
        let patch = text(
            select(diff.as_bytes(), &[LineRef { old: None, new: Some(2) }, LineRef { old: Some(2), new: None }], false)
                .unwrap(),
        );
        assert_eq!(patch, "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\r\n-b\r\n\\ No newline at end of file\n+c\r\n\\ No newline at end of file\n");
    }
}
