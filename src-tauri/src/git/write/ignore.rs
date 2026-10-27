//! Ignoring files from Local changes: a pattern added to the `.gitignore` at the top of the repository, under the
//! rules of `write/mod.rs`. Only the file is written; git runs nothing, so a folder does not need trusting for it.
//! A file git already tracks stays tracked: ignoring is for untracked files.

use std::io::Write as _;
use std::path::Path;

use super::{queue, record, Activity, Writer};
use crate::git::cli::top;

/// A `.gitignore` line from the window: one line, not a comment, not an exception (`!`), not `..`.
fn pattern(line: &str) -> Result<&str, String> {
    let line = line.trim();
    let bad = line.is_empty()
        || line.starts_with('#')
        || line.starts_with('!')
        || line.contains(['\n', '\r', '\0'])
        || line.split('/').any(|part| part == "..");
    if bad {
        return Err(said!("ignore.badLine", "\"{line}\" cannot go in .gitignore.", line = line));
    }
    Ok(line)
}

/// The patterns `file` lacks, and the text that adds them (starting on a new line when the file does not end in one).
fn addition(file: &str, patterns: &[&str]) -> (Vec<String>, String) {
    let known: Vec<&str> = file.lines().map(str::trim).collect();
    let mut new: Vec<String> = Vec::new();
    for found in patterns {
        if !known.contains(found) && !new.iter().any(|known| known == found) {
            new.push((*found).to_owned());
        }
    }
    if new.is_empty() {
        return (new, String::new());
    }
    let mut text = String::new();
    if !file.is_empty() && !file.ends_with('\n') {
        text.push('\n');
    }
    for line in &new {
        text.push_str(line);
        text.push('\n');
    }
    (new, text)
}

impl Writer<'_> {
    /// Adds `patterns` (relative to the opened folder, as the window shows paths) to the repository's top `.gitignore`;
    /// a pattern it has already is left out.
    pub fn ignore(&self, patterns: &[String]) -> Result<(), String> {
        let patterns: Vec<&str> = patterns.iter().map(|line| pattern(line)).collect::<Result<_, _>>()?;
        if patterns.is_empty() {
            return Err(said!("ignore.nothing", "Nothing to ignore."));
        }
        let (toplevel, prefix) = top(self.root)?;
        // An anchored pattern ("/src/a.ts") is relative to the folder the .gitignore is in: the top, so the opened
        // folder's place goes in front of it. One without a slash ("*.log") matches at any depth, as it is.
        let placed: Vec<String> = patterns
            .iter()
            .map(|line| match line.strip_prefix('/') {
                Some(rest) => format!("/{prefix}{rest}"),
                None => (*line).to_owned(),
            })
            .collect();
        let placed: Vec<&str> = placed.iter().map(String::as_str).collect();
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let file = Path::new(&toplevel).join(".gitignore");
        if file.is_symlink() {
            return Err(said!("ignore.link", "This repository's .gitignore is a link: Haunt leaves it alone."));
        }
        let before = std::fs::read_to_string(&file).unwrap_or_default();
        let (new, text) = addition(&before, &placed);
        if new.is_empty() {
            return Ok(());
        }
        let mut out =
            std::fs::OpenOptions::new().create(true).append(true).open(&file).map_err(|error| {
                said!("ignore.notWritten", ".gitignore could not be written: {error}", error = error)
            })?;
        out.write_all(text.as_bytes())
            .map_err(|error| said!("ignore.notWritten", ".gitignore could not be written: {error}", error = error))?;
        record(
            self.root,
            Activity {
                command: format!("add to .gitignore: {}", new.join(", ")),
                at: super::now_seconds(),
                ms: 0,
                ok: true,
                output: String::new(),
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_one_plain_line() {
        assert_eq!(pattern(" *.log "), Ok("*.log"));
        assert_eq!(pattern("/build/"), Ok("/build/"));
        for bad in ["", "# note", "!keep", "a\nb", "../up", "a/../b"] {
            assert!(pattern(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn adds_only_what_is_not_there_on_a_new_line() {
        assert_eq!(addition("", &["*.log"]), (vec!["*.log".to_owned()], "*.log\n".to_owned()));
        assert_eq!(addition("node_modules", &["*.log", "node_modules"]).1, "\n*.log\n");
        assert_eq!(addition("*.log\n", &["*.log"]).0.len(), 0);
        assert_eq!(addition("a\n", &["b", "b"]).1, "b\n");
    }
}
