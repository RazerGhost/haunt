//! The `git` program, read-only, for what Haunt leaves to git itself: the log pages, the status count, churn, blame and
//! a function's history (`git log -L`). A repository's own config could make git run commands (a diff driver, a
//! textconv, a clean or smudge filter, fsmonitor, a pager), so every call switches those off: reading a folder you have
//! not trusted runs nothing of its own here either. Optional locks are off too, so a read never takes the index lock
//! from a git you are running yourself. Your own git config (system and global) stays: it holds core.autocrlf on
//! Windows, without which every line of a checked-out file would differ from the repository. The settings themselves are
//! in `command.rs`, with those of the writes.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a repository's list of filters is used before it is read again. Its `.git/config` is watched (repos.rs),
/// but a filter can also come from an included file, `config.worktree` or your global config, which are not: read
/// again this often, a filter added there is switched off within these seconds.
const FILTERS_FRESH: Duration = Duration::from_secs(3);

/// The filters each repository's config names, switched off for every call; read again after `FILTERS_FRESH`, and
/// forgotten when its config changes or its tab is shown or closed (repos.rs).
type Filters = HashMap<String, (Instant, Vec<String>)>;

fn filters() -> &'static Mutex<Filters> {
    static FILTERS: OnceLock<Mutex<Filters>> = OnceLock::new();
    FILTERS.get_or_init(Mutex::default)
}

/// Reads the repository's filters again on the next call.
pub fn forget_config(root: &str) {
    filters().lock().remove(root);
}

/// The longest a read may take: a pickaxe search or blame over a huge history is stopped rather than holding on for ever.
const READ_LIMIT: Duration = Duration::from_secs(300);

fn spawn(command: Command) -> Result<Vec<u8>, String> {
    let output = crate::process::run(command, READ_LIMIT).map_err(|failure| match failure {
        crate::process::Failure::NotFound => said!("cli.notInstalled", "git could not start: it is not installed."),
        crate::process::Failure::TimedOut => {
            said!(
                "cli.tookTooLong",
                "git took longer than {minutes} minutes and was stopped.",
                minutes = READ_LIMIT.as_secs() / 60
            )
        }
        crate::process::Failure::Other(error) => {
            said!("cli.couldNotStart", "git could not start: {error}", error = error)
        }
    })?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if message.is_empty() {
            said!("cli.failed", "git failed ({status})", status = output.status)
        } else {
            message
        })
    }
}

/// The filters the repository's config names (to be switched off), as last read or read now.
fn filters_of(root: &str) -> Vec<String> {
    let known =
        filters().lock().get(root).filter(|(read, _)| read.elapsed() < FILTERS_FRESH).map(|(_, names)| names.clone());
    if let Some(names) = known {
        return names;
    }
    let listing = spawn(super::command::read(root, &[], "config", &["--get-regexp", r"^filter\."]));
    let out = listing.map(|out| String::from_utf8_lossy(&out).into_owned()).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for line in out.lines() {
        let key = line.split_whitespace().next().unwrap_or_default();
        // filter.<name>.<setting>: the name may itself hold dots.
        if let Some(rest) = key.strip_prefix("filter.") {
            if let Some((name, _)) = rest.rsplit_once('.') {
                if !names.iter().any(|known| known == name) {
                    names.push(name.to_owned());
                }
            }
        }
    }
    filters().lock().insert(root.to_owned(), (Instant::now(), names.clone()));
    names
}

/// Runs a read-only git command in the repository at `root`, with everything that could run its own commands off.
pub fn git(root: &str, args: &[&str]) -> Result<String, String> {
    git_bytes(root, args).map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// As `git`, with the output as it is: a patch must keep every byte (`git/write`).
pub fn git_bytes(root: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    spawn(command(root, args)?)
}

/// A read's git: the repository's filters off, then the command with the flags it needs to run nothing.
fn command(root: &str, args: &[&str]) -> Result<Command, String> {
    let (command, rest) = args.split_first().ok_or_else(|| said!("cli.noCommand", "No git command."))?;
    Ok(super::command::read(root, &filters_of(root), command, rest))
}

/// A read whose output is taken a line at a time, as it comes (a long log, every object of the history), rather than
/// kept whole; as safe as `git`. git is stopped when this is dropped.
pub struct Lines {
    children: Vec<Child>,
    reader: BufReader<ChildStdout>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    done: bool,
}

impl Lines {
    /// The next line without its line end; at the end, git's error once if it failed; then None.
    pub fn next_line(&mut self) -> Option<Result<String, String>> {
        if self.done {
            return None;
        }
        let mut line = Vec::new();
        match self.reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => {
                self.done = true;
                let mut failed = None;
                for child in &mut self.children {
                    if let Ok(status) = child.wait() {
                        if !status.success() {
                            failed.get_or_insert(status);
                        }
                    }
                }
                let stderr = self.stderr.take().and_then(|handle| handle.join().ok()).unwrap_or_default();
                failed.map(|status| {
                    let message = String::from_utf8_lossy(&stderr).trim().to_owned();
                    Err(if message.is_empty() {
                        said!("cli.failed", "git failed ({status})", status = status)
                    } else {
                        message
                    })
                })
            }
            Ok(_) => {
                while line.last().is_some_and(|byte| *byte == b'\n' || *byte == b'\r') {
                    line.pop();
                }
                Some(Ok(String::from_utf8_lossy(&line).into_owned()))
            }
        }
    }
}

impl Drop for Lines {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn started(command: &mut Command) -> Result<Child, String> {
    crate::startup::hide_console(command);
    command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => said!("cli.notInstalled", "git could not start: it is not installed."),
        _ => said!("cli.couldNotStart", "git could not start: {error}", error = error),
    })
}

/// `args` read a line at a time.
pub fn stream(root: &str, args: &[&str]) -> Result<Lines, String> {
    let mut command = command(root, args)?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = started(&mut command)?;
    lines_of(vec![], child.stdout.take(), child.stderr.take(), child)
}

/// `first`'s output fed to `second` (`rev-list --objects | cat-file --batch-check`), read a line at a time.
pub fn pipe(root: &str, first: &[&str], second: &[&str]) -> Result<Lines, String> {
    let mut upstream = command(root, first)?;
    upstream.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut feeding = started(&mut upstream)?;
    let mut command = command(root, second)?;
    let input = feeding.stdout.take().ok_or_else(|| said!("cli.noOutput", "git gave no output."))?;
    command.stdin(Stdio::from(input)).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = started(&mut command)?;
    lines_of(vec![feeding], child.stdout.take(), child.stderr.take(), child)
}

fn lines_of(
    mut children: Vec<Child>,
    stdout: Option<ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    last: Child,
) -> Result<Lines, String> {
    children.push(last);
    let stdout = stdout.ok_or_else(|| said!("cli.noOutput", "git gave no output."))?;
    // Read aside, so git never waits on a full error pipe while its output is read.
    let stderr = stderr.map(|mut stderr| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stderr.read_to_end(&mut out);
            out
        })
    });
    Ok(Lines { children, reader: BufReader::new(stdout), stderr, done: false })
}

/// The repository's top folder, and the opened folder's place in it ("" at the top, "packages/app/" deeper in).
pub fn top(root: &str) -> Result<(String, String), String> {
    let out = git(root, &["rev-parse", "--show-toplevel", "--show-prefix"])?;
    let mut lines = out.lines();
    let toplevel = lines.next().unwrap_or_default().trim().to_owned();
    let prefix = lines.next().unwrap_or_default().trim().to_owned();
    if toplevel.is_empty() {
        return Err(said!("cli.notRepository", "{root} is not in a git repository.", root = root));
    }
    Ok((toplevel, prefix))
}

/// A branch, tag or commit name from the window: never an option, never a range.
pub fn revision(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || name.starts_with('-')
        || name.contains(char::is_whitespace)
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(said!("cli.notRevision", "\"{name}\" is not a branch, tag or commit.", name = name));
    }
    Ok(name)
}
