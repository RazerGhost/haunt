//! Fetch, pull and push: under the rules of `write/mod.rs`.
//!
//! - Fetch runs in any folder. In an untrusted one, what the repository's own config could make it run is switched
//!   off: the `ext::` transport, its own `core.sshCommand`, credential helpers, `core.gitProxy` and `core.askPass`
//!   (`untrusted_fetch`), `remote.*.uploadpack` (each remote is fetched with git's own `--upload-pack`), hooks
//!   (`reference-transaction`), submodules and automatic maintenance. Your own settings from your global and system
//!   config stay.
//! - Pull and push run only in a trusted folder: pull checks files out (filters) and push runs the `pre-push` hook.
//! - Pull fast-forwards; when the branches went apart it says so, and the window asks whether to merge or rebase. A
//!   pull that stops on conflicts leaves them for the conflict view (`conflicts.rs`), where it is continued or aborted.
//! - Push sets the upstream the first time, and overwrites only with a lease on what Haunt last fetched.
//! - git's questions (passwords, passphrases, host keys) go to the window (`crate::askpass`); git's progress goes to
//!   `progress` as it comes; a run can be stopped (`cancel`), which stops git and everything it started.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{now_seconds, queue, record, Activity, Writer};
use crate::git::cli::git as read;

/// How a pull joins the remote branch's commits with yours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PullMode {
    /// Only when your branch has nothing the remote one lacks.
    FastForward,
    Merge,
    Rebase,
}

/// What a pull did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PullResult {
    UpToDate,
    Pulled,
    /// Both sides have commits the other lacks: merge or rebase (asked).
    Diverged,
    /// The merge or rebase stopped on conflicts, left in place for the conflict view.
    Conflicts,
}

/// What a push did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PushResult {
    Pushed,
    /// The remote has commits you do not: pull first, or overwrite them (asked).
    Rejected,
    /// The branch tracks nothing yet and there are several remotes: which one (asked).
    ChooseRemote {
        remotes: Vec<String>,
    },
}

/// What a network run of git said.
pub(super) struct Ran {
    pub(super) ok: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) cancelled: bool,
}

/// A fetch's `-c` settings and environment.
type FetchSetup = (Vec<String>, Vec<(String, String)>);

/// How many pull requests a fetch of them brings in, newest first (by number); an older one is fetched by its number.
const PULLS_FETCHED: usize = 30;

/// What fetching a remote's pull requests found.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PullFetch {
    pub remote: String,
    /// How many pull requests (or merge requests) the remote has, open or closed.
    pub total: u32,
    /// How many were fetched now.
    pub fetched: u32,
}

/// The pull requests in `git ls-remote`'s answer: each one's number and its ref on the remote.
fn pull_refs(listing: &str) -> Vec<(u32, String)> {
    listing
        .lines()
        .filter_map(|line| {
            let reference = line.split('\t').nth(1)?.trim();
            let number = reference
                .strip_prefix("refs/pull/")
                .or_else(|| reference.strip_prefix("refs/merge-requests/"))?
                .strip_suffix("/head")?
                .parse()
                .ok()?;
            Some((number, reference.to_owned()))
        })
        .collect()
}

/// The network run going on per repository: git's process id, and whether it was stopped.
type Running = Mutex<HashMap<String, (u32, Arc<AtomicBool>)>>;

fn runs() -> &'static Running {
    static RUNNING: OnceLock<Running> = OnceLock::new();
    RUNNING.get_or_init(Mutex::default)
}

/// One network run at a time per repository: a fetch, pull or push holds it (a foreground one together with the write
/// queue, taken first; the background fetch alone, so your own writes never wait for it).
pub(super) fn network_turn(root: &str) -> Arc<Mutex<()>> {
    static TURNS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    TURNS.get_or_init(Mutex::default).lock().unwrap().entry(root.to_owned()).or_default().clone()
}

/// Whether a fetch, pull or push is running in `root`.
pub fn running(root: &str) -> bool {
    runs().lock().unwrap().contains_key(root)
}

/// Stops the fetch, pull or push running in `root`, with every process git started (a credential helper, ssh).
pub fn cancel(root: &str) -> bool {
    let Some((pid, stopped)) = runs().lock().unwrap().get(root).cloned() else {
        return false;
    };
    stopped.store(true, Ordering::SeqCst);
    #[cfg(windows)]
    let mut kill = {
        let mut command = Command::new("taskkill");
        command.args(["/T", "/F", "/PID", &pid.to_string()]);
        command
    };
    #[cfg(not(windows))]
    let mut kill = {
        let mut command = Command::new("kill");
        command.args(["-TERM", &format!("-{pid}")]);
        command
    };
    kill.stdout(Stdio::null()).stderr(Stdio::null());
    crate::startup::hide_console(&mut kill);
    kill.status().is_ok()
}

/// A line of git's progress ("Receiving objects:  45% (12/27)"), as opposed to what it has to say.
fn is_progress(line: &str) -> bool {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map(str::trim_start).unwrap_or(line);
    line.contains("objects:") || line.contains("deltas:") || line.starts_with("Total ") || line.is_empty()
}

/// What git said, without its progress: the message for the window.
pub(super) fn said(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|line| !is_progress(line)).collect();
    let text = lines.join("\n").trim().to_owned();
    if text.is_empty() {
        said!("network.silent", "git failed without saying why.")
    } else {
        text
    }
}

/// One entry of `git config --list --show-scope -z`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Setting {
    pub scope: String,
    pub key: String,
    pub value: Option<String>,
}

/// Reads `git config --list --show-scope -z`: "scope NUL key LF value NUL", or "scope NUL key NUL" without a value.
pub(super) fn settings(listing: &str) -> Vec<Setting> {
    let mut parts = listing.split('\0');
    let mut out = Vec::new();
    while let (Some(scope), Some(entry)) = (parts.next(), parts.next()) {
        let (key, value) = match entry.split_once('\n') {
            Some((key, value)) => (key, Some(value.to_owned())),
            None => (entry, None),
        };
        out.push(Setting { scope: scope.to_owned(), key: key.to_owned(), value });
    }
    out
}

/// What an untrusted fetch adds to stop the repository's own config from running anything: `-c` settings and
/// environment variables. Settings from your system and global config are kept.
pub(super) fn untrusted_fetch(all: &[Setting]) -> (Vec<String>, Vec<(String, String)>) {
    let mine = |setting: &&Setting| setting.scope == "local" || setting.scope == "worktree";
    let theirs = |setting: &&Setting| setting.scope == "system" || setting.scope == "global";
    // Section and variable names come lower case from git; a subsection (a URL, a remote's name) keeps its case.
    let named = |setting: &Setting, name: &str| setting.key.to_lowercase() == name;
    let is_helper = |setting: &Setting| {
        let lower = setting.key.to_lowercase();
        lower.starts_with("credential.") && lower.ends_with(".helper")
    };
    let mut config: Vec<String> =
        ["gc.auto=0", "maintenance.auto=false", "fetch.recurseSubmodules=false", "submodule.recurse=false"]
            .iter()
            .map(|setting| setting.to_string())
            .collect();
    let mut env = Vec::new();
    let repository = all.iter().filter(mine).collect::<Vec<_>>();

    if repository.iter().any(|setting| is_helper(setting)) {
        // An empty helper empties the list git has gathered; yours come back after it, in their order.
        config.push("credential.helper=".into());
        for setting in all.iter().filter(theirs).filter(|setting| is_helper(setting)) {
            config.push(format!("{}={}", setting.key, setting.value.as_deref().unwrap_or_default()));
        }
    }
    if repository.iter().any(|setting| named(setting, "core.sshcommand")) {
        let yours = all.iter().filter(theirs).rfind(|setting| named(setting, "core.sshcommand"));
        let command = yours.and_then(|setting| setting.value.clone()).unwrap_or_else(|| "ssh".into());
        env.push(("GIT_SSH_COMMAND".into(), command));
    }
    if repository.iter().any(|setting| named(setting, "core.gitproxy")) {
        // Set but empty: git uses no proxy command and reads none from the config.
        env.push(("GIT_PROXY_COMMAND".into(), String::new()));
    }
    if repository.iter().any(|setting| named(setting, "core.askpass")) {
        config.push("core.askPass=".into());
    }
    if repository.iter().any(|setting| named(setting, "core.alternaterefscommand")) {
        config.push("core.alternateRefsCommand=exit 0".into());
    }
    (config, env)
}

impl Writer<'_> {
    /// Runs a network git command: git's questions go to the window, its progress to `progress`, and it can be stopped.
    pub(super) fn network(
        &self,
        args: &[&str],
        extra: &[String],
        env: &[(String, String)],
        progress: &dyn Fn(&str),
    ) -> Result<Ran, String> {
        let mut command = Command::new("git");
        command
            .args(self.config())
            .args(extra.iter().flat_map(|setting| ["-c".to_string(), setting.clone()]))
            .args(args)
            .current_dir(self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", ":")
            .env("GIT_SEQUENCE_EDITOR", ":")
            .env("LC_ALL", "C")
            .envs(if self.quiet { Vec::new() } else { crate::askpass::env(if self.tabless { "" } else { self.root }) })
            .envs(self.quiet.then_some(("GCM_INTERACTIVE", "never")))
            .envs(env.iter().map(|(name, value)| (name.as_str(), value.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::startup::hide_console(&mut command);
        // Its own process group, so stopping it stops what it started too.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let started = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|error| said!("network.couldNotStart", "git could not start: {error}", error = error))?;
        let stopped = Arc::new(AtomicBool::new(false));
        runs().lock().unwrap().insert(self.root.to_owned(), (child.id(), stopped.clone()));

        let mut stdout = child.stdout.take();
        let reading = std::thread::spawn(move || {
            let mut out = String::new();
            if let Some(stdout) = stdout.as_mut() {
                let _ = stdout.read_to_string(&mut out);
            }
            out
        });
        // git rewrites a progress line in place with \r; each piece is shown as it comes, at most ten times a second.
        let mut stderr = Vec::new();
        let mut piece = Vec::new();
        let mut shown = Instant::now() - std::time::Duration::from_secs(1);
        if let Some(mut pipe) = child.stderr.take() {
            let mut buffer = [0u8; 4096];
            while let Ok(count) = pipe.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                for &byte in &buffer[..count] {
                    if byte == b'\r' || byte == b'\n' {
                        let line = String::from_utf8_lossy(&piece).into_owned();
                        if !line.trim().is_empty() && shown.elapsed().as_millis() >= 100 {
                            progress(line.trim());
                            shown = Instant::now();
                        }
                        // Only finished lines are kept; a progress line rewritten in place is not.
                        if byte == b'\n' {
                            stderr.extend_from_slice(&piece);
                            stderr.push(b'\n');
                        }
                        piece.clear();
                    } else {
                        piece.push(byte);
                    }
                }
            }
        }
        stderr.extend_from_slice(&piece);
        let status = child.wait().map_err(|error| error.to_string());
        runs().lock().unwrap().remove(self.root);
        let stdout = reading.join().unwrap_or_default();
        let status = status?;
        let stderr = String::from_utf8_lossy(&stderr).into_owned();
        let cancelled = stopped.load(Ordering::SeqCst);
        record(
            self.root,
            Activity {
                command: format!("git {}", args.join(" ")),
                at: now_seconds(),
                ms: started.elapsed().as_millis() as u32,
                ok: status.success() && !cancelled,
                output: format!("{}\n{}", stdout.trim(), said(&stderr)).trim().chars().take(4000).collect(),
            },
        );
        Ok(Ran { ok: status.success() && !cancelled, stdout, stderr, cancelled })
    }

    fn remotes(&self) -> Result<Vec<String>, String> {
        Ok(read(self.root, &["remote"])?
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub(super) fn current_branch(&self) -> Result<String, String> {
        let name = read(self.root, &["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default().trim().to_owned();
        if name.is_empty() {
            return Err(said!(
                "network.detached",
                "You are not on a branch (HEAD is detached). Switch to a branch first."
            ));
        }
        Ok(name)
    }

    fn setting(&self, key: &str) -> Option<String> {
        read(self.root, &["config", "--get", key])
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    pub(super) fn git_path_exists(&self, name: &str) -> bool {
        read(self.root, &["rev-parse", "--git-path", name])
            .is_ok_and(|path| std::path::Path::new(self.root).join(path.trim()).exists())
    }

    pub(super) fn head(&self) -> String {
        read(self.root, &["rev-parse", "-q", "--verify", "HEAD"]).unwrap_or_default().trim().to_owned()
    }

    /// Fetches every remote, pruning remote branches that are gone. Your branches and files stay as they are.
    pub fn fetch(&self, progress: &dyn Fn(&str)) -> Result<(), String> {
        let remotes = self.remotes()?;
        if remotes.is_empty() {
            return Err(said!("network.noRemoteToFetch", "This repository has no remote to fetch from."));
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        self.fetch_remotes(&remotes, progress)
    }

    /// A fetch nobody asked for right now (Settings, Git: fetch in the background): it asks nothing, and it does not
    /// wait: while another write or fetch runs in the repository it is skipped. None when skipped (no remote, busy),
    /// else how the fetch went.
    pub fn fetch_in_background(&self) -> Option<Result<(), String>> {
        let quiet = Writer { quiet: true, ..*self };
        let remotes = quiet.remotes().ok()?;
        if remotes.is_empty() || running(self.root) {
            return None;
        }
        // Only the network turn: staging, committing and the other writes do not wait for a fetch nobody asked for.
        let network = network_turn(self.root);
        let _network = network.try_lock().ok()?;
        Some(quiet.fetch_remotes(&remotes, &|_| {}))
    }

    /// The `-c` settings and environment a fetch from this repository runs with: none when it is trusted; otherwise
    /// those that keep its own config from running anything.
    fn fetch_setup(&self) -> Result<FetchSetup, String> {
        if self.trusted {
            return Ok((Vec::new(), Vec::new()));
        }
        let listing = read(self.root, &["config", "--list", "--show-scope", "--includes", "-z"])?;
        let (extra, mut env) = untrusted_fetch(&settings(&listing));
        env.retain(|(name, _)| std::env::var_os(name).is_none());
        Ok((extra, env))
    }

    /// Fetches a remote's pull requests as refs (GitHub's and Gitea's `refs/pull/<n>/head`, GitLab's
    /// `refs/merge-requests/<n>/head`; no account or API), into Haunt's own `refs/haunt/pull/<remote>/<n>`, which no
    /// branch list shows: the newest `PULLS_FETCHED` by number, and `number` too when asked for. `remote` is origin, or
    /// the only remote, when not named.
    pub fn fetch_pull_requests(
        &self,
        remote: Option<&str>,
        number: Option<u32>,
        progress: &dyn Fn(&str),
    ) -> Result<PullFetch, String> {
        let remotes = self.remotes()?;
        let remote = match remote {
            Some(name) if remotes.iter().any(|known| known == name) => name.to_owned(),
            Some(name) => {
                return Err(said!("network.noSuchRemote", "This repository has no remote \"{name}\".", name = name))
            }
            None if remotes.iter().any(|known| known == "origin") => "origin".to_owned(),
            None => remotes
                .first()
                .cloned()
                .ok_or_else(|| said!("network.noRemoteToFetch", "This repository has no remote to fetch from."))?,
        };
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        let (extra, env) = self.fetch_setup()?;
        let listed = self.network(
            &[
                "ls-remote",
                "--upload-pack=git-upload-pack",
                "--end-of-options",
                &remote,
                "refs/pull/*/head",
                "refs/merge-requests/*/head",
            ],
            &extra,
            &env,
            progress,
        )?;
        if listed.cancelled {
            return Err(said!("network.fetchStopped", "Fetching was stopped."));
        }
        if !listed.ok {
            return Err(said(&listed.stderr));
        }
        let mut found = pull_refs(&listed.stdout);
        let total = found.len() as u32;
        if found.is_empty() {
            return Err(said!(
                "network.noPullRefs",
                "{remote} offers no pull requests as refs. GitHub, GitLab and Gitea do; other hosts may not.",
                remote = remote
            ));
        }
        found.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
        let mut wanted: Vec<(u32, String)> = found.iter().take(PULLS_FETCHED).cloned().collect();
        if let Some(number) = number {
            let Some(one) = found.iter().find(|(n, _)| *n == number) else {
                return Err(said!(
                    "network.noPull",
                    "{remote} has no pull request #{number}.",
                    remote = remote,
                    number = number
                ));
            };
            if !wanted.iter().any(|(n, _)| *n == number) {
                wanted.push(one.clone());
            }
        }
        let specs: Vec<String> =
            wanted.iter().map(|(n, source)| format!("+{source}:refs/haunt/pull/{remote}/{n}")).collect();
        let mut args = vec![
            "fetch",
            "--progress",
            "--no-tags",
            "--no-recurse-submodules",
            "--upload-pack=git-upload-pack",
            "--end-of-options",
            remote.as_str(),
        ];
        args.extend(specs.iter().map(String::as_str));
        let ran = self.network(&args, &extra, &env, progress)?;
        if ran.cancelled {
            return Err(said!("network.fetchStopped", "Fetching was stopped."));
        }
        if !ran.ok {
            return Err(said(&ran.stderr));
        }
        Ok(PullFetch { remote, total, fetched: wanted.len() as u32 })
    }

    fn fetch_remotes(&self, remotes: &[String], progress: &dyn Fn(&str)) -> Result<(), String> {
        if self.trusted {
            let ran = self.network(&["fetch", "--all", "--prune", "--progress"], &[], &[], progress)?;
            if ran.cancelled {
                return Err(said!("network.fetchStopped", "Fetching was stopped."));
            }
            return if ran.ok { Ok(()) } else { Err(said(&ran.stderr)) };
        }
        let listing = read(self.root, &["config", "--list", "--show-scope", "--includes", "-z"])?;
        let (extra, mut env) = untrusted_fetch(&settings(&listing));
        // Your own GIT_SSH_COMMAND, set where Haunt was started, already wins over any config.
        env.retain(|(name, _)| std::env::var_os(name).is_none());
        // One remote at a time, each with git's own upload-pack: a repository's `remote.*.uploadpack` would win over a
        // `-c` setting (git takes the first), and `fetch --all` does not pass `--upload-pack` on to each remote.
        let mut failed = Vec::new();
        for remote in remotes {
            let args = [
                "fetch",
                "--prune",
                "--progress",
                "--no-recurse-submodules",
                "--upload-pack=git-upload-pack",
                "--end-of-options",
                remote.as_str(),
            ];
            let ran = self.network(&args, &extra, &env, progress)?;
            if ran.cancelled {
                return Err(said!("network.fetchStopped", "Fetching was stopped."));
            }
            if !ran.ok {
                failed.push(if remotes.len() > 1 {
                    format!("{remote}: {}", crate::said::english(&said(&ran.stderr)))
                } else {
                    said(&ran.stderr)
                });
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed.join("\n"))
        }
    }

    /// Pulls the current branch's upstream into it: fast-forward, or merge or rebase as asked.
    pub fn pull(&self, mode: PullMode, progress: &dyn Fn(&str)) -> Result<PullResult, String> {
        if !self.trusted {
            return Err(said!(
                "network.pullUntrusted",
                "Pulling checks files out and can run this repository's hooks and filters, so it runs only in a trusted folder. Fetch works here; trust this folder to pull."
            ));
        }
        let branch = self.current_branch()?;
        if read(self.root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).is_err() {
            return Err(said!(
                "network.noUpstream",
                "{branch} tracks no remote branch, so there is nothing to pull. Push it first to set one up.",
                branch = branch
            ));
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        self.wait_for_lock()?;
        let before = self.head();
        let args: &[&str] = match mode {
            PullMode::FastForward => &["pull", "--ff-only", "--progress"],
            PullMode::Merge => &["pull", "--no-rebase", "--no-edit", "--progress"],
            PullMode::Rebase => &["pull", "--rebase", "--progress"],
        };
        let ran = self.network(args, &[], &[], progress)?;
        if ran.ok {
            return Ok(if self.head() == before { PullResult::UpToDate } else { PullResult::Pulled });
        }
        // Stopped on conflicts: they stay, for the conflict view (continue or abort there).
        if self.operation()?.is_some_and(|going| !going.conflicted.is_empty()) {
            return Ok(PullResult::Conflicts);
        }
        if ran.cancelled {
            return Err(said!("network.pullStopped", "Pulling was stopped."));
        }
        if mode == PullMode::FastForward && ran.stderr.contains("Not possible to fast-forward") {
            return Ok(PullResult::Diverged);
        }
        Err(said(&ran.stderr))
    }

    /// Checks the submodules out at the commits the repository records, cloning the ones not there yet (`paths`, or all
    /// when empty). Only in a trusted folder: it clones from the URLs in `.gitmodules` and checks files out.
    pub fn update_submodules(&self, paths: &[String], progress: &dyn Fn(&str)) -> Result<(), String> {
        if !self.trusted {
            return Err(said!(
                "network.submodulesUntrusted",
                "Updating submodules clones from the addresses in this repository's .gitmodules and checks files out, so it runs only in a trusted folder. Trust this folder to update them."
            ));
        }
        let paths = paths.iter().map(|path| crate::git::local::checked(path)).collect::<Result<Vec<_>, _>>()?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        self.wait_for_lock()?;
        let mut args = vec!["submodule", "update", "--init", "--progress", "--"];
        args.extend(paths);
        let ran = self.network(&args, &[], &[], progress)?;
        if ran.cancelled {
            return Err(said!("network.submodulesStopped", "Updating the submodules was stopped."));
        }
        if !ran.ok {
            return Err(said(&ran.stderr));
        }
        Ok(())
    }

    /// Which remote a push goes to: the one asked for (it must exist), or the only one; none means ask.
    fn push_remote(&self, asked: Option<&str>) -> Result<Option<String>, String> {
        let remotes = self.remotes()?;
        if remotes.is_empty() {
            return Err(said!("network.noRemoteToPush", "This repository has no remote to push to."));
        }
        Ok(match asked {
            Some(name) if remotes.iter().any(|remote| remote == name) => Some(name.to_owned()),
            Some(name) => return Err(said!("network.noRemoteNamed", "There is no remote named {name}.", name = name)),
            None if remotes.len() == 1 => remotes.into_iter().next(),
            None => None,
        })
    }

    /// Pushes the current branch to the branch it tracks, or, the first time, to a branch of the same name that it
    /// then tracks. `force` overwrites the remote branch, only if it is still where Haunt last fetched it.
    pub fn push(&self, remote: Option<&str>, force: bool, progress: &dyn Fn(&str)) -> Result<PushResult, String> {
        if !self.trusted {
            return Err(said!(
                "network.pushUntrusted",
                "Pushing runs this repository's pre-push hook, so it runs only in a trusted folder. Trust this folder to push."
            ));
        }
        let branch = self.current_branch()?;
        let tracked =
            match (self.setting(&format!("branch.{branch}.remote")), self.setting(&format!("branch.{branch}.merge"))) {
                (Some(upstream), Some(merge)) if upstream != "." && remote.is_none_or(|asked| asked == upstream) => {
                    Some((upstream, merge.strip_prefix("refs/heads/").unwrap_or(&merge).to_owned()))
                }
                _ => None,
            };
        let (remote, target, first) = match tracked {
            Some((upstream, target)) => (upstream, target, false),
            None => match self.push_remote(remote)? {
                Some(remote) => (remote, branch.clone(), true),
                None => return Ok(PushResult::ChooseRemote { remotes: self.remotes()? }),
            },
        };
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        let mut args = vec!["push".to_string(), "--progress".to_string()];
        if first {
            args.push("--set-upstream".into());
        }
        if force {
            // The lease: the remote branch must still be where this repository last saw it (nowhere, if it never did).
            let seen = read(self.root, &["rev-parse", "-q", "--verify", &format!("refs/remotes/{remote}/{target}")])
                .unwrap_or_default()
                .trim()
                .to_owned();
            args.push(format!("--force-with-lease=refs/heads/{target}:{seen}"));
        }
        args.push("--end-of-options".into());
        args.push(remote.clone());
        args.push(format!("refs/heads/{branch}:refs/heads/{target}"));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let ran = self.network(&args, &[], &[], progress)?;
        if ran.ok {
            return Ok(PushResult::Pushed);
        }
        if ran.cancelled {
            return Err(said!("network.pushStopped", "Pushing was stopped."));
        }
        if ran.stderr.contains("stale info") {
            return Err(said!(
                "network.stale",
                "{remote}/{target} has changed since this repository last fetched it, so it was not overwritten. Fetch and look at what is there first.",
                remote = remote,
                target = target
            ));
        }
        if ran.stderr.contains("[rejected]") {
            return Ok(PushResult::Rejected);
        }
        Err(said(&ran.stderr))
    }

    /// Pushes a tag to a remote (the only one, or the one asked for).
    pub fn push_tag(&self, name: &str, remote: Option<&str>, progress: &dyn Fn(&str)) -> Result<PushResult, String> {
        if !self.trusted {
            return Err(said!(
                "network.pushUntrusted",
                "Pushing runs this repository's pre-push hook, so it runs only in a trusted folder. Trust this folder to push."
            ));
        }
        let name = self.tag_name(name)?;
        if read(self.root, &["rev-parse", "-q", "--verify", &format!("refs/tags/{name}")]).is_err() {
            return Err(said!("network.noTag", "There is no tag {name}.", name = name));
        }
        let Some(remote) = self.push_remote(remote)? else {
            return Ok(PushResult::ChooseRemote { remotes: self.remotes()? });
        };
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        let network = network_turn(self.root);
        let _network = network.lock().unwrap();
        let spec = format!("refs/tags/{name}:refs/tags/{name}");
        let ran = self.network(&["push", "--progress", "--end-of-options", &remote, &spec], &[], &[], progress)?;
        if ran.ok {
            return Ok(PushResult::Pushed);
        }
        if ran.cancelled {
            return Err(said!("network.pushStopped", "Pushing was stopped."));
        }
        if ran.stderr.contains("[rejected]") || ran.stderr.contains("already exists") {
            return Err(said!(
                "network.tagThere",
                "{remote} already has a tag {name} on another commit; it was left as it is.",
                remote = remote,
                name = name
            ));
        }
        Err(said(&ran.stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(scope: &str, key: &str, value: &str) -> Setting {
        Setting { scope: scope.into(), key: key.into(), value: Some(value.into()) }
    }

    #[test]
    fn reads_the_config_listing_with_scopes() {
        let listing = "system\0filter.lfs.clean\ngit-lfs clean -- %f\0global\0core.bare\0local\0remote.origin.url\nhttps://example.com/a.git\0";
        assert_eq!(
            settings(listing),
            vec![
                setting("system", "filter.lfs.clean", "git-lfs clean -- %f"),
                Setting { scope: "global".into(), key: "core.bare".into(), value: None },
                setting("local", "remote.origin.url", "https://example.com/a.git"),
            ]
        );
    }

    #[test]
    fn an_untrusted_fetch_switches_off_what_the_repository_would_run_and_keeps_yours() {
        let all = vec![
            setting("system", "credential.helper", "manager"),
            setting("global", "credential.https://dev.example.com.helper", "store"),
            setting("global", "core.sshcommand", "C:/Windows/System32/OpenSSH/ssh.exe"),
            setting("local", "credential.helper", "!touch marker"),
            setting("local", "core.sshcommand", "sh -c 'touch marker'"),
            setting("local", "remote.Origin.uploadpack", "touch marker; git-upload-pack"),
            setting("worktree", "core.gitproxy", "touch-marker"),
            setting("local", "core.askpass", "touch-marker"),
            setting("command", "credential.helper", "from-haunt-itself"),
        ];
        let (config, env) = untrusted_fetch(&all);
        let helpers: Vec<&str> =
            config.iter().map(String::as_str).filter(|setting| setting.contains("helper")).collect();
        assert_eq!(
            helpers,
            ["credential.helper=", "credential.helper=manager", "credential.https://dev.example.com.helper=store"]
        );
        assert!(config.contains(&"core.askPass=".to_string()));
        assert!(config.contains(&"gc.auto=0".to_string()));
        assert!(env.contains(&("GIT_SSH_COMMAND".into(), "C:/Windows/System32/OpenSSH/ssh.exe".into())));
        assert!(env.contains(&("GIT_PROXY_COMMAND".into(), String::new())));

        // Nothing of the repository's own: your settings are left alone.
        let (config, env) = untrusted_fetch(&all[..3]);
        assert!(!config.iter().any(|setting| setting.contains("helper")));
        assert!(env.is_empty());
        // The repository's ssh command and no one of yours: plain ssh.
        let (_, env) = untrusted_fetch(&[setting("local", "core.sshcommand", "evil")]);
        assert_eq!(env, vec![("GIT_SSH_COMMAND".into(), "ssh".into())]);
    }

    #[test]
    fn keeps_what_git_said_without_its_progress() {
        let stderr = "remote: Enumerating objects: 5, done.\nReceiving objects: 100% (5/5), done.\n\
                      To ../remote.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs\n";
        assert_eq!(
            said(stderr),
            "To ../remote.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs"
        );
        assert_eq!(
            crate::said::english(&said("Receiving objects: 100% (5/5), done.\n")),
            "git failed without saying why."
        );
    }
}
