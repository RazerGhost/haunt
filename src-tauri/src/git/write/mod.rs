//! Writing to a repository: the only place in Haunt that runs a git command that writes.
//! A fixed list of operations, each with its own arguments: stage, unstage, a few lines of either, discard
//! (whole files or lines) with a way back, and commit or amend; branches, stashes and tags in `refs.rs`; fetch, pull
//! and push in `network.rs`; revert, cherry-pick, reset and undoing a commit in `undo.rs`; merging and conflicts in
//! `conflicts.rs`; interactive rebase in `rebase.rs`. Paths are
//! checked (inside the opened folder, never an option) and always follow `--`.
//!
//! - Writes run one at a time per repository, and wait a moment when another git holds `index.lock`; Haunt never
//!   deletes a lock.
//! - A trusted folder runs git as a terminal would: hooks, filters, signing. An untrusted one runs none of its own: hooks
//!   point at an empty folder, fsmonitor and signing are off, and a file with a filter (Git LFS) is refused, since git
//!   would store it the wrong way without its filter.
//! - A discard is saved first, as a commit under `refs/haunt/discarded/<time>` (made through a temporary index, so
//!   your index is left alone; untracked files and deletions included), and can be brought back for 30 days.
//! - Every command lands in the repository's activity list, with how long it took and what git said.

mod bisect;
mod conflicts;
mod create;
mod last;
mod network;
mod patch;
mod patches;
pub(super) mod rebase;
mod refs;
mod remotes;
mod undo;
mod worktrees;

use std::collections::{HashMap, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

pub use bisect::{bisect_state, BisectMark, BisectState};
pub use conflicts::{ConflictFile, Operation, Side};
pub use create::{clone, init};
pub use last::{forget_last, LastAction, Snapshot, Undo};
pub use network::{cancel, PullFetch, PullMode, PullResult, PushResult};
pub use patch::LineRef;
pub use patches::PatchPreview;
pub use rebase::RebaseStep;
pub use refs::{DroppedStash, SwitchMode};
pub use undo::{Outcome, ResetMode};
pub use worktrees::{worktrees, Worktree};

use super::cli::{git as read, git_bytes, top};
use super::local::{checked, tracked, tracked_of};

/// A temporary index of Haunt's own (`GIT_INDEX_FILE`), so a save, a restore or a preview leaves your index alone;
/// its file is removed when this is dropped, however the work ended.
pub(super) struct TempIndex {
    path: String,
}

impl TempIndex {
    pub(super) fn new(label: &str) -> TempIndex {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("haunt-{label}-{}-{count}", std::process::id()));
        TempIndex { path: path.to_string_lossy().into_owned() }
    }

    /// git's environment for it: the index, and Haunt as the author and committer of what is made from it.
    pub(super) fn env(&self) -> [(&'static str, &str); 5] {
        [
            ("GIT_INDEX_FILE", self.path.as_str()),
            ("GIT_AUTHOR_NAME", "Haunt"),
            ("GIT_AUTHOR_EMAIL", "haunt@localhost"),
            ("GIT_COMMITTER_NAME", "Haunt"),
            ("GIT_COMMITTER_EMAIL", "haunt@localhost"),
        ]
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// How long a write waits for another git's `index.lock`.
const LOCK_WAIT: Duration = Duration::from_secs(3);
/// How long a discard can be brought back.
const KEEP_DISCARDS: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const DISCARDED: &str = "refs/haunt/discarded/";

/// A command Haunt ran to write, for the activity list.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Activity {
    pub command: String,
    /// Seconds since 1970.
    pub at: i64,
    pub ms: u32,
    pub ok: bool,
    /// What git said (cut at a few kilobytes).
    pub output: String,
}

/// A discard that can be brought back.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Discarded {
    /// Its time in milliseconds, which names it.
    pub id: String,
    /// Seconds since 1970.
    pub at: i64,
    pub paths: Vec<String>,
}

/// What lines of a file a lines operation works on, and what it does with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum LinesAction {
    /// Lines of the unstaged changes into the index.
    Stage,
    /// Lines of the staged changes out of the index.
    Unstage,
    /// Lines of the unstaged changes thrown away (saved first).
    Discard,
}

/// What the commit box needs to know: whether hooks and signing will run, and the last message, for amending.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitBox {
    pub trusted: bool,
    /// The repository's commit hooks (pre-commit, commit-msg…), which run only in a trusted folder.
    pub hooks: Vec<String>,
    /// Commits are signed (commit.gpgsign), which happens only in a trusted folder.
    pub signing: bool,
    /// The message of the last commit; null before the first one.
    pub last_message: Option<String>,
}

fn now_seconds() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_secs() as i64).unwrap_or(0)
}

/// One lock per repository: its writes run one at a time.
pub(crate) fn queue(root: &str) -> Arc<Mutex<()>> {
    static QUEUES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    QUEUES.get_or_init(Mutex::default).lock().unwrap().entry(root.to_owned()).or_default().clone()
}

fn activities() -> &'static Mutex<HashMap<String, VecDeque<Activity>>> {
    static ACTIVITY: OnceLock<Mutex<HashMap<String, VecDeque<Activity>>>> = OnceLock::new();
    ACTIVITY.get_or_init(Mutex::default)
}

/// The repository's activity, newest first.
pub fn activity(root: &str) -> Vec<Activity> {
    activities().lock().unwrap().get(root).map(|list| list.iter().rev().cloned().collect()).unwrap_or_default()
}

fn record(root: &str, entry: Activity) {
    let mut all = activities().lock().unwrap();
    let list = all.entry(root.to_owned()).or_default();
    list.push_back(entry);
    while list.len() > 100 {
        list.pop_front();
    }
}

/// An empty folder for `core.hooksPath` in an untrusted folder: no hook can run from it.
fn no_hooks() -> PathBuf {
    let dir = std::env::temp_dir().join("haunt-no-hooks");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A repository to write to, as the shown tab has it: its folder and whether its own code may run.
pub struct Writer<'a> {
    root: &'a str,
    trusted: bool,
    /// Run without asking anything (a background fetch): git gets no askpass, and Git Credential Manager no window.
    quiet: bool,
    /// Not a repository of a tab (the folder a clone goes into): git's questions are not tied to a tab.
    tabless: bool,
}

impl<'a> Writer<'a> {
    pub fn new(root: &'a str, trusted: bool) -> Writer<'a> {
        Writer { root, trusted, quiet: false, tabless: false }
    }

    fn config(&self) -> Vec<String> {
        let mut out: Vec<String> =
            ["color.ui=false", "core.pager=cat"].iter().map(|setting| setting.to_string()).collect();
        if !self.trusted {
            out.push(format!("core.hooksPath={}", no_hooks().to_string_lossy().replace('\\', "/")));
            out.extend(
                ["core.fsmonitor=false", "commit.gpgsign=false", "tag.gpgsign=false", "protocol.ext.allow=never"]
                    .iter()
                    .map(|setting| setting.to_string()),
            );
        }
        out.into_iter().flat_map(|setting| ["-c".to_string(), setting]).collect()
    }

    /// Runs a writing git command in `cwd` (the opened folder by default), with `input` on its stdin, and records it.
    fn git(
        &self,
        args: &[&str],
        input: Option<&[u8]>,
        env: &[(&str, &str)],
        cwd: Option<&str>,
    ) -> Result<String, String> {
        let mut command = Command::new("git");
        command
            .args(self.config())
            .args(args)
            .current_dir(cwd.unwrap_or(self.root))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", ":")
            .env("GIT_SEQUENCE_EDITOR", ":")
            .env("LC_ALL", "C")
            .envs(env.iter().copied())
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::startup::hide_console(&mut command);
        let started = Instant::now();
        let mut child = command.spawn().map_err(|error| format!("git could not start: {error}"))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            let _ = stdin.write_all(input);
        }
        let output = child.wait_with_output().map_err(|error| error.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let mut said = format!("{}\n{stderr}", stdout.trim()).trim().to_owned();
        if said.len() > 4000 {
            let mut cut = 4000;
            while !said.is_char_boundary(cut) {
                cut -= 1;
            }
            said.truncate(cut);
            said.push('…');
        }
        record(
            self.root,
            Activity {
                command: format!("git {}", args.join(" ")),
                at: now_seconds(),
                ms: started.elapsed().as_millis() as u32,
                ok: output.status.success(),
                output: said,
            },
        );
        if output.status.success() {
            Ok(stdout)
        } else if stderr.is_empty() {
            Err(format!("git failed ({})", output.status))
        } else {
            Err(stderr)
        }
    }

    /// Waits while another git holds the index lock; Haunt never deletes the lock itself.
    fn wait_for_lock(&self) -> Result<(), String> {
        let lock = read(self.root, &["rev-parse", "--git-path", "index.lock"])?;
        let lock = Path::new(self.root).join(lock.trim());
        let started = Instant::now();
        while lock.exists() {
            if started.elapsed() > LOCK_WAIT {
                return Err("Another git is busy in this repository (it holds .git/index.lock). Try again when it is \
                            done; Haunt leaves the lock alone."
                    .into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    /// In an untrusted folder, a file with a filter (Git LFS and the like) cannot be written: git would store it
    /// without its filter, the wrong way.
    fn refuse_filtered(&self, paths: &[&str], always: bool) -> Result<(), String> {
        if (self.trusted && !always) || paths.is_empty() {
            return Ok(());
        }
        let out = read(self.root, &[&["check-attr", "filter", "--"][..], paths].concat())?;
        for line in out.lines() {
            // "path: filter: value"
            let mut parts = line.rsplitn(2, ": ");
            let value = parts.next().unwrap_or_default().trim();
            if value != "unspecified" && value != "unset" {
                let path = parts.next().unwrap_or_default().trim_end_matches(": filter");
                return Err(if self.trusted {
                    format!("{path} goes through a filter ({value}): take the whole file.")
                } else {
                    format!("{path} goes through a filter ({value}), which runs only in a trusted folder. Trust this folder to write it.")
                });
            }
        }
        Ok(())
    }

    fn has_head(&self) -> bool {
        read(self.root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).is_ok_and(|out| !out.trim().is_empty())
    }

    /* ---------- staging ---------- */

    /// Stages whole files (new, changed or deleted).
    pub fn stage(&self, paths: &[String]) -> Result<(), String> {
        let paths = checked_all(paths)?;
        self.refuse_filtered(&paths, false)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.git(&[&["add", "-A", "--"][..], &paths].concat(), None, &[], None).map(drop)
    }

    /// Takes whole files out of the index, back to how the last commit has them.
    pub fn unstage(&self, paths: &[String]) -> Result<(), String> {
        let paths = checked_all(paths)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        if self.has_head() {
            self.git(&[&["restore", "--staged", "--"][..], &paths].concat(), None, &[], None).map(drop)
        } else {
            // Before the first commit there is nothing to go back to: the files leave the index.
            self.git(&[&["rm", "--cached", "-r", "-q", "--"][..], &paths].concat(), None, &[], None).map(drop)
        }
    }

    /// Stages, unstages or discards a few lines of one file.
    pub fn lines(&self, path: &str, picked: &[LineRef], action: LinesAction) -> Result<(), String> {
        let path = checked(path)?;
        if action != LinesAction::Unstage && !tracked(self.root, path)? {
            return Err("Lines of a new file cannot be picked: take the whole file.".into());
        }
        self.refuse_filtered(&[path], true)?;
        let (toplevel, prefix) = top(self.root)?;
        let full = format!("{prefix}{path}");
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        let staged = action == LinesAction::Unstage;
        let mut args = vec!["diff", "--no-color", "-U3"];
        if staged {
            args.push("--cached");
        }
        args.extend(["--", full.as_str()]);
        let diff = git_bytes(&toplevel, &args)?;
        let patch = patch::select(&diff, picked, action != LinesAction::Stage)?
            .ok_or("None of those lines is a change any more.")?;
        let apply: &[&str] = match action {
            LinesAction::Stage => &["apply", "--cached", "-"],
            LinesAction::Unstage => &["apply", "--cached", "--reverse", "-"],
            LinesAction::Discard => {
                self.save(&[path], "Discarded lines")?;
                &["apply", "--reverse", "-"]
            }
        };
        self.git(apply, Some(&patch), &[], Some(&toplevel)).map(drop)
    }

    /* ---------- discarding ---------- */

    /// Throws away the unstaged changes of whole files (an untracked file is deleted), after saving them.
    pub fn discard(&self, paths: &[String]) -> Result<(), String> {
        let paths = checked_all(paths)?;
        self.refuse_filtered(&paths, false)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.save(&paths, "Discarded")?;
        let mut known = Vec::new();
        let tracked_now = tracked_of(self.root, &paths)?;
        for path in &paths {
            if tracked_now.contains(*path) {
                known.push(*path);
            } else {
                std::fs::remove_file(Path::new(self.root).join(path)).map_err(|error| format!("{path}: {error}"))?;
            }
        }
        if !known.is_empty() {
            self.git(&[&["restore", "--worktree", "--"][..], &known].concat(), None, &[], None)?;
        }
        Ok(())
    }

    /// Saves the working tree's version of `paths` (deleted ones as deleted) as a commit under refs/haunt/discarded,
    /// through a temporary index; removes saves older than 30 days.
    fn save(&self, paths: &[&str], title: &str) -> Result<(), String> {
        let head = self.has_head();
        // What there is to save: files on disk, and files the last commit has (saved as deleted when they are gone).
        let committed = if head {
            read(self.root, &[&["ls-tree", "-r", "--name-only", "HEAD", "--"][..], paths].concat())?
        } else {
            String::new()
        };
        let paths: Vec<&str> = paths
            .iter()
            .copied()
            .filter(|path| Path::new(self.root).join(path).exists() || committed.lines().any(|line| line == *path))
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        let paths = paths.as_slice();
        // The save's name is its time; a later one in the same millisecond takes the next free one, never replacing it.
        let mut stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_millis()).unwrap_or(0);
        while read(self.root, &["rev-parse", "--verify", "--quiet", &format!("{DISCARDED}{stamp}")]).is_ok() {
            stamp += 1;
        }
        let index = TempIndex::new("index");
        let env = index.env();
        let result = (|| {
            self.git(if head { &["read-tree", "HEAD"] } else { &["read-tree", "--empty"] }, None, &env, None)?;
            self.git(&[&["add", "-A", "--"][..], paths].concat(), None, &env, None)?;
            let tree = self.git(&["write-tree"], None, &env, None)?.trim().to_owned();
            let message = format!("{title} by Haunt\n\n{}\n", paths.join("\n"));
            let mut args = vec!["commit-tree", "--no-gpg-sign", tree.as_str()];
            if head {
                args.extend(["-p", "HEAD"]);
            }
            let commit = self.git(&args, Some(message.as_bytes()), &env, None)?.trim().to_owned();
            self.git(&["update-ref", &format!("{DISCARDED}{stamp}"), &commit], None, &[], None).map(drop)
        })();
        drop(index);
        result?;
        let cutoff = stamp.saturating_sub(KEEP_DISCARDS.as_millis());
        for old in self.discarded()? {
            if old.id.parse::<u128>().is_ok_and(|time| time < cutoff) {
                let _ = self.git(&["update-ref", "-d", &format!("{DISCARDED}{}", old.id)], None, &[], None);
            }
        }
        Ok(())
    }

    /// The saved discards, newest first.
    pub fn discarded(&self) -> Result<Vec<Discarded>, String> {
        let out = read(self.root, &["for-each-ref", "--format=%(refname)%00%(contents:body)%00%01", DISCARDED])?;
        let mut list: Vec<Discarded> = out
            .split('\u{1}')
            .filter_map(|entry| {
                let mut parts = entry.trim_start_matches('\n').split('\0');
                let id = parts.next()?.strip_prefix(DISCARDED)?.to_owned();
                let paths = parts
                    .next()
                    .unwrap_or_default()
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect();
                let at = id.parse::<i64>().map(|ms| ms / 1000).unwrap_or(0);
                Some(Discarded { id, at, paths })
            })
            .collect();
        list.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(list)
    }

    /// Brings a discard back: its files as they were, after saving what is there now (so this can be undone too).
    pub fn restore(&self, id: &str) -> Result<(), String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err("That discard is not known.".into());
        }
        let reference = format!("{DISCARDED}{id}");
        let saved = self.discarded()?.into_iter().find(|entry| entry.id == id).ok_or("That discard is not known.")?;
        let paths = checked_all(&saved.paths)?;
        self.refuse_filtered(&paths, false)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        self.save(&paths, "Replaced on restoring")?;
        // The paths the save has; the others were deleted when it was made, and are deleted again.
        let present = read(self.root, &[&["ls-tree", "-r", "--name-only", &reference, "--"][..], &paths].concat())?;
        let present: Vec<&str> = present.lines().collect();
        let (back, gone): (Vec<&str>, Vec<&str>) = paths.iter().partition(|path| present.contains(path));
        if !back.is_empty() {
            // Written out through a temporary index holding the save, so your index is left alone and untracked files
            // come back untracked; checkout-index applies line endings and filters as git does.
            let index = TempIndex::new("restore");
            let env = index.env();
            self.git(&["read-tree", &reference], None, &env, None)?;
            self.git(&[&["checkout-index", "-f", "--"][..], &back].concat(), None, &env, None)?;
        }
        for path in gone {
            let file = Path::new(self.root).join(path);
            if file.exists() {
                std::fs::remove_file(&file).map_err(|error| format!("{path}: {error}"))?;
            }
        }
        self.git(&["update-ref", "-d", &reference], None, &[], None).map(drop)
    }

    /* ---------- committing ---------- */

    /// What the commit box shows before you commit.
    pub fn commit_box(&self) -> Result<CommitBox, String> {
        let hooks_dir = read(self.root, &["rev-parse", "--git-path", "hooks"]).unwrap_or_default();
        let configured = read(self.root, &["config", "--get", "core.hooksPath"]).unwrap_or_default();
        let dir = if configured.trim().is_empty() { hooks_dir.trim().to_owned() } else { configured.trim().to_owned() };
        let dir = Path::new(self.root).join(dir);
        let hooks = ["pre-commit", "prepare-commit-msg", "commit-msg", "post-commit"]
            .iter()
            .filter(|name| dir.join(name).is_file())
            .map(|name| name.to_string())
            .collect();
        let signing =
            read(self.root, &["config", "--bool", "--get", "commit.gpgsign"]).is_ok_and(|out| out.trim() == "true");
        let last_message = self
            .has_head()
            .then(|| read(self.root, &["log", "-1", "--format=%B"]).ok())
            .flatten()
            .map(|message| message.trim_end().to_owned());
        Ok(CommitBox { trusted: self.trusted, hooks, signing, last_message })
    }

    /// Commits what is staged with `message`, or amends the last commit; returns the new commit.
    pub fn commit(&self, message: &str, amend: bool, signoff: bool) -> Result<String, String> {
        if message.trim().is_empty() {
            return Err("Write a message first.".into());
        }
        if amend && !self.has_head() {
            return Err("There is no commit to amend yet.".into());
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.wait_for_lock()?;
        // Whitespace only: a line of the message that starts with # is part of it.
        let mut args = vec!["commit", "--cleanup=whitespace", "-F", "-"];
        if amend {
            args.push("--amend");
        }
        if signoff {
            args.push("--signoff");
        }
        self.git(&args, Some(message.as_bytes()), &[], None)?;
        Ok(read(self.root, &["rev-parse", "HEAD"])?.trim().to_owned())
    }
}

fn checked_all(paths: &[String]) -> Result<Vec<&str>, String> {
    if paths.is_empty() {
        return Err("No files given.".into());
    }
    paths.iter().map(|path| checked(path)).collect()
}

#[cfg(test)]
mod tests;
