//! Branches, stashes and tags: the writes, under the rules of `write/mod.rs` (one
//! write at a time per repository, the index lock waited on and never deleted, every command in the activity list).
//!
//! - Names from the window are checked with `git check-ref-format`, and revisions with `revision()`, and always follow
//!   `--end-of-options`.
//! - Switching branches, stashing and applying a stash write files into the working tree; in an untrusted folder whose
//!   files go through filters (Git LFS), git would write those files the wrong way without them, so they are refused
//!   there until the folder is trusted.
//! - A dropped stash is kept under `refs/haunt/dropped/<time>` and can be brought back for 30 days; deleting a branch
//!   whose commits are not merged anywhere asks first (`DeleteBranch::Merged` refuses, `Unmerged` goes ahead).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{queue, Writer};
use crate::git::cli::{git as read, revision};

const DROPPED: &str = "refs/haunt/dropped/";
const KEEP_DROPPED: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What switching does with the changes in the working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum SwitchMode {
    /// They come along (git refuses when the other branch changes the same files).
    Carry,
    /// They are stashed first, untracked files too, and stay in the stash list.
    Stash,
}

/// A dropped stash that can be brought back.
#[derive(Debug, Clone, Serialize, Type)]
pub struct DroppedStash {
    /// Its time in milliseconds, which names it.
    pub id: String,
    /// Seconds since 1970.
    pub at: i64,
    pub message: String,
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_millis()).unwrap_or(0)
}

impl Writer<'_> {
    /// A branch name from the window, as git takes it (`git check-ref-format --branch`).
    pub(super) fn branch_name<'n>(&self, name: &'n str) -> Result<&'n str, String> {
        let bad = name.is_empty() || name.starts_with('-') || name.contains(char::is_whitespace);
        if bad || read(self.root, &["check-ref-format", "--branch", name]).is_err() {
            return Err(format!("\"{name}\" cannot be a branch name."));
        }
        Ok(name)
    }

    pub(super) fn tag_name<'n>(&self, name: &'n str) -> Result<&'n str, String> {
        let bad = name.is_empty() || name.starts_with('-') || name.contains(char::is_whitespace);
        if bad || read(self.root, &["check-ref-format", &format!("refs/tags/{name}")]).is_err() {
            return Err(format!("\"{name}\" cannot be a tag name."));
        }
        Ok(name)
    }

    /// In an untrusted folder, a write that checks files out is refused when the repository's files go through a filter
    /// (`filter=` in a `.gitattributes`, `info/attributes` or `core.attributesFile`) and a filter driver is set up. A
    /// filter defined only in git's own config (Git for Windows sets up Git LFS for everyone) is not enough.
    pub(super) fn refuse_checkout(&self, what: &str) -> Result<(), String> {
        if self.trusted || !self.uses_filters() {
            return Ok(());
        }
        Err(format!(
            "{what} writes files that go through this repository's filters (such as Git LFS), which run only in a \
             trusted folder. Trust this folder first."
        ))
    }

    fn uses_filters(&self) -> bool {
        let drivers =
            read(self.root, &["config", "--get-regexp", r"^filter\..*\.(clean|smudge|process)$"]).unwrap_or_default();
        if drivers.trim().is_empty() {
            return false;
        }
        let mut files: Vec<std::path::PathBuf> =
            read(self.root, &["ls-files", "-z", "--", ".gitattributes", "*/.gitattributes"])
                .unwrap_or_default()
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(|path| std::path::Path::new(self.root).join(path))
                .collect();
        if let Ok(info) = read(self.root, &["rev-parse", "--path-format=absolute", "--git-path", "info/attributes"]) {
            files.push(info.trim().into());
        }
        if let Ok(global) = read(self.root, &["config", "--path", "core.attributesFile"]) {
            files.push(global.trim().into());
        }
        files
            .iter()
            .filter_map(|file| std::fs::read_to_string(file).ok())
            .any(|text| text.lines().any(|line| !line.trim_start().starts_with('#') && line.contains("filter=")))
    }

    /* ---------- branches ---------- */

    /// Creates a branch at `at` (a branch, tag or commit; HEAD by default), and switches to it when asked.
    pub fn create_branch(&self, name: &str, at: Option<&str>, switch: bool) -> Result<(), String> {
        let name = self.branch_name(name)?;
        let at = at.map(revision).transpose()?.unwrap_or("HEAD");
        if switch {
            self.refuse_checkout("Switching branches")?;
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        if switch {
            self.git(&["switch", "--no-track", "-c", name, "--end-of-options", at], None, &[], None).map(drop)
        } else {
            self.git(&["branch", "--no-track", "--end-of-options", name, at], None, &[], None).map(drop)
        }
    }

    /// Switches to a local branch, or to a remote branch through a local one that tracks it (made when there is none).
    pub fn switch_branch(&self, name: &str, mode: SwitchMode) -> Result<(), String> {
        let name = revision(name)?;
        self.refuse_checkout("Switching branches")?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let local = read(self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_ok();
        let remote =
            !local && read(self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{name}")]).is_ok();
        if !local && !remote {
            return Err(format!("There is no branch called \"{name}\"."));
        }
        if mode == SwitchMode::Stash {
            let dirty = !read(self.root, &["status", "--porcelain", "--untracked-files=normal"])?.trim().is_empty();
            if dirty {
                let message = format!("Before switching to {name}");
                self.git(&["stash", "push", "--include-untracked", "-m", &message], None, &[], None)?;
            }
        }
        if local {
            return self.git(&["switch", "--no-guess", "--end-of-options", name], None, &[], None).map(drop);
        }
        // origin/feature: a local feature that tracks it, or the one that already exists.
        let short = name.split_once('/').map_or(name, |(_, rest)| rest);
        let short = self.branch_name(short)?;
        if read(self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{short}")]).is_ok() {
            return self.git(&["switch", "--no-guess", "--end-of-options", short], None, &[], None).map(drop);
        }
        self.git(&["switch", "-c", short, "--track", &format!("refs/remotes/{name}")], None, &[], None).map(drop)
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<(), String> {
        let from = self.branch_name(from)?;
        let to = self.branch_name(to)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.git(&["branch", "-m", "--end-of-options", from, to], None, &[], None).map(drop)
    }

    /// The commits of a branch that no other branch, remote branch or tag has: what deleting it would lose.
    pub fn unmerged(&self, name: &str) -> Result<u32, String> {
        let name = self.branch_name(name)?;
        // Applied to --branches, the pattern names the branch without refs/heads/.
        let own = format!("--exclude={name}");
        let out = read(
            self.root,
            &["rev-list", "--count", &format!("refs/heads/{name}"), "--not", &own, "--branches", "--remotes", "--tags"],
        )?;
        Ok(out.trim().parse().unwrap_or(0))
    }

    /// Deletes a local branch; one with commits no other ref has only when `unmerged` says so (the window asks).
    pub fn delete_branch(&self, name: &str, unmerged: bool) -> Result<(), String> {
        let name = self.branch_name(name)?;
        if !unmerged && self.unmerged(name)? > 0 {
            return Err(format!(
                "{name} has commits no other branch has. Deleting it loses them; confirm to go ahead."
            ));
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.git(&["branch", "-D", "--end-of-options", name], None, &[], None).map(drop)
    }

    /* ---------- stashes ---------- */

    /// Stashes the changes in the working tree (untracked files too when asked), with an optional message.
    /// Stashes the changes, or only those of `paths` (relative to the opened folder; untracked ones need `untracked`).
    pub fn stash(&self, message: Option<&str>, untracked: bool, paths: &[String]) -> Result<(), String> {
        self.refuse_checkout("Stashing")?;
        let paths = if paths.is_empty() { Vec::new() } else { super::checked_all(paths)? };
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let mut args = vec!["stash", "push"];
        if untracked {
            args.push("--include-untracked");
        }
        if let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) {
            args.extend(["-m", message]);
        }
        if !paths.is_empty() {
            args.push("--");
            args.extend(paths);
        }
        let out = self.git(&args, None, &[], None)?;
        if out.contains("No local changes to save") {
            return Err("There are no changes to stash.".into());
        }
        Ok(())
    }

    fn stash_ref(index: u32) -> String {
        format!("stash@{{{index}}}")
    }

    /// Applies a stash to the working tree; `pop` also drops it when it applied cleanly.
    pub fn apply_stash(&self, index: u32, pop: bool) -> Result<(), String> {
        self.refuse_checkout("Applying a stash")?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let reference = Self::stash_ref(index);
        self.git(&["stash", if pop { "pop" } else { "apply" }, &reference], None, &[], None).map(drop)
    }

    /// Drops a stash, keeping it under refs/haunt/dropped so it can be brought back; removes keeps older than 30 days.
    pub fn drop_stash(&self, index: u32) -> Result<(), String> {
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let reference = Self::stash_ref(index);
        let sha = read(self.root, &["rev-parse", "--verify", "--quiet", &reference])?.trim().to_owned();
        if sha.is_empty() {
            return Err("That stash is not there any more.".into());
        }
        let mut stamp = now_ms();
        while read(self.root, &["rev-parse", "--verify", "--quiet", &format!("{DROPPED}{stamp}")]).is_ok() {
            stamp += 1;
        }
        self.git(&["update-ref", &format!("{DROPPED}{stamp}"), &sha], None, &[], None)?;
        self.git(&["stash", "drop", &reference], None, &[], None)?;
        let cutoff = stamp.saturating_sub(KEEP_DROPPED.as_millis());
        for old in self.dropped()? {
            if old.id.parse::<u128>().is_ok_and(|time| time < cutoff) {
                let _ = self.git(&["update-ref", "-d", &format!("{DROPPED}{}", old.id)], None, &[], None);
            }
        }
        Ok(())
    }

    /// The dropped stashes that can be brought back, newest first.
    pub fn dropped(&self) -> Result<Vec<DroppedStash>, String> {
        let out = read(self.root, &["for-each-ref", "--format=%(refname)%00%(contents:subject)", DROPPED])?;
        let mut list: Vec<DroppedStash> = out
            .lines()
            .filter_map(|line| {
                let (reference, message) = line.split_once('\0')?;
                let id = reference.strip_prefix(DROPPED)?.to_owned();
                let at = id.parse::<i64>().map(|ms| ms / 1000).unwrap_or(0);
                Some(DroppedStash { id, at, message: message.to_owned() })
            })
            .collect();
        list.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(list)
    }

    /// Brings a dropped stash back into the stash list.
    pub fn restore_stash(&self, id: &str) -> Result<(), String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err("That stash is not known.".into());
        }
        let reference = format!("{DROPPED}{id}");
        let saved = self.dropped()?.into_iter().find(|entry| entry.id == id).ok_or("That stash is not known.")?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let sha = read(self.root, &["rev-parse", "--verify", "--quiet", &reference])?.trim().to_owned();
        self.git(&["stash", "store", "-m", &saved.message, &sha], None, &[], None)?;
        self.git(&["update-ref", "-d", &reference], None, &[], None).map(drop)
    }

    /* ---------- tags ---------- */

    /// Creates a tag at `at` (HEAD by default): annotated when it has a message, lightweight otherwise.
    pub fn create_tag(&self, name: &str, at: Option<&str>, message: Option<&str>) -> Result<(), String> {
        let name = self.tag_name(name)?;
        let at = at.map(revision).transpose()?.unwrap_or("HEAD");
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        match message.map(str::trim).filter(|message| !message.is_empty()) {
            Some(message) => {
                self.git(&["tag", "-a", "-F", "-", "--end-of-options", name, at], Some(message.as_bytes()), &[], None)
            }
            None => self.git(&["tag", "--end-of-options", name, at], None, &[], None),
        }
        .map(drop)
    }

    pub fn delete_tag(&self, name: &str) -> Result<(), String> {
        let name = self.tag_name(name)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.git(&["tag", "-d", "--end-of-options", name], None, &[], None).map(drop)
    }
}
