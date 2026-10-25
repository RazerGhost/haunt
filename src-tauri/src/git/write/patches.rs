//! Patches: commits saved as patch files (`git format-patch`), and patch files applied, as commits (`git am`) or as
//! changes in the working tree (`git apply`). Before applying, a preview: the patches are applied to a copy of the last
//! commit in a temporary index and made into a commit that no branch points at (git cleans such commits up), so the
//! window can show what they change function by function without touching your files, index or branches.
//!
//! Patch files are named by the person through a file dialog: absolute paths, never an option.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use super::{queue, Writer};
use crate::git::cli::{git as read, revision};

/// A patch file, as it reads.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PatchInfo {
    pub file: String,
    /// From its Subject line (without `[PATCH 1/3]`); the file's name for a plain diff.
    pub subject: String,
    /// From its From line; empty for a plain diff.
    pub author: String,
    /// The files it changes.
    pub paths: Vec<String>,
}

/// The patches, and how they would apply.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PatchPreview {
    pub patches: Vec<PatchInfo>,
    /// A commit on top of the last one with the patches applied, on no branch, to compare with HEAD; null when they do
    /// not apply.
    pub commit: Option<String>,
    /// What git said when they do not apply.
    pub problem: Option<String>,
}

/// A patch file's path, checked: absolute, an existing file, never an option.
fn patch_file(path: &str) -> Result<&str, String> {
    let file = Path::new(path);
    if path.starts_with('-') || !file.is_absolute() || !file.is_file() {
        return Err(format!("\"{path}\" is not a patch file."));
    }
    Ok(path)
}

/// Reads a patch file's subject, author and files, from its mail headers and `diff --git` lines.
pub fn read_patch(path: &str) -> Result<PatchInfo, String> {
    let text = std::fs::read_to_string(patch_file(path)?).map_err(|error| format!("{path}: {error}"))?;
    let name = Path::new(path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut subject = String::new();
    let mut author = String::new();
    let mut paths = Vec::new();
    let mut in_subject = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Subject: ").filter(|_| subject.is_empty()) {
            subject = rest.to_owned();
            in_subject = true;
            continue;
        }
        // A long subject goes on over lines that start with a space.
        if in_subject && line.starts_with(' ') {
            subject.push_str(line);
            continue;
        }
        in_subject = false;
        if let Some(rest) = line.strip_prefix("From: ").filter(|_| author.is_empty()) {
            author = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("diff --git a/") {
            if let Some((_, to)) = rest.split_once(" b/") {
                if !paths.iter().any(|seen| seen == to) {
                    paths.push(to.to_owned());
                }
            }
        }
    }
    // "[PATCH 2/3] Fix it" → "Fix it".
    let subject = match subject.trim().strip_prefix('[').and_then(|rest| rest.split_once(']')) {
        Some((tag, rest)) if tag.to_ascii_uppercase().contains("PATCH") => rest.trim().to_owned(),
        _ => subject.trim().to_owned(),
    };
    if paths.is_empty() {
        return Err(format!("{name} changes no file: it is not a patch."));
    }
    Ok(PatchInfo { file: path.to_owned(), subject: if subject.is_empty() { name } else { subject }, author, paths })
}

impl Writer<'_> {
    /// Saves `to` (with `from`: every commit after `from` up to `to`) as patch files in `folder`; returns their paths.
    pub fn save_patches(&self, from: Option<&str>, to: &str, folder: &str) -> Result<Vec<String>, String> {
        let to = revision(to)?;
        let dir = Path::new(folder);
        if folder.starts_with('-') || !dir.is_absolute() || !dir.is_dir() {
            return Err(format!("\"{folder}\" is not a folder."));
        }
        let range = match from {
            Some(from) => format!("{}..{to}", revision(from)?),
            None => to.to_owned(),
        };
        let mut args = vec!["format-patch", "-o", folder];
        if from.is_none() {
            args.push("-1");
        }
        args.extend(["--end-of-options", range.as_str(), "--"]);
        let out = self.git(&args, None, &[], None)?;
        Ok(out.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect())
    }

    /// Reads the patches and tries them on a copy of the last commit, leaving your files, index and branches alone.
    pub fn preview_patches(&self, files: &[String]) -> Result<PatchPreview, String> {
        let patches = files.iter().map(|file| read_patch(file)).collect::<Result<Vec<_>, _>>()?;
        let head = read(self.root, &["rev-parse", "--verify", "--quiet", "HEAD"]).map(|out| out.trim().to_owned());
        let Ok(head) = head else {
            return Ok(PatchPreview {
                patches,
                commit: None,
                problem: Some("This repository has no commit yet.".into()),
            });
        };
        let index = super::TempIndex::new("patches");
        let env = index.env();
        let result = (|| -> Result<Result<String, String>, String> {
            self.git(&["read-tree", &head], None, &env, None)?;
            for file in files {
                if let Err(problem) = self.git(&["apply", "--cached", "--", file], None, &env, None) {
                    let name = Path::new(file).file_name().map(|name| name.to_string_lossy().into_owned());
                    return Ok(Err(format!("{} does not apply: {problem}", name.unwrap_or_default())));
                }
            }
            let tree = self.git(&["write-tree"], None, &env, None)?.trim().to_owned();
            let commit = self.git(
                &["commit-tree", "--no-gpg-sign", &tree, "-p", &head],
                Some(b"Patches previewed by Haunt\n"),
                &env,
                None,
            )?;
            Ok(Ok(commit.trim().to_owned()))
        })();
        Ok(match result? {
            Ok(commit) => PatchPreview { patches, commit: Some(commit), problem: None },
            Err(problem) => PatchPreview { patches, commit: None, problem: Some(problem) },
        })
    }

    /// Applies the patches: as commits on the current branch (`git am`), or as changes in the working tree only (`git
    /// apply`). Either all apply or none: a failed `am` is undone at once.
    pub fn apply_patches(&self, files: &[String], commit: bool) -> Result<(), String> {
        for file in files {
            patch_file(file)?;
        }
        if files.is_empty() {
            return Err("Choose a patch file first.".into());
        }
        self.refuse_checkout("Applying patches")?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let paths: Vec<&str> = files.iter().map(String::as_str).collect();
        if commit {
            if self.git_path_exists("rebase-apply") {
                return Err("A rebase or another patch is going on: finish or stop it first.".into());
            }
            let args = [&["am", "--3way", "--"][..], &paths].concat();
            if let Err(problem) = self.git(&args, None, &[], None) {
                let _ = self.git(&["am", "--abort"], None, &[], None);
                return Err(format!("{problem}\nNothing was applied."));
            }
            Ok(())
        } else {
            self.git(&[&["apply", "--"][..], &paths].concat(), None, &[], None).map(drop)
        }
    }
}
