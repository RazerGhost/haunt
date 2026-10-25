//! What Haunt needs on the machine: a JavaScript runtime for the TypeScript scan (Node, else Bun, else Deno) and git
//! (2.24 or later). Checked at start, so a missing one is said plainly at once instead of failing later.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The lowest Node that runs TypeScript files without flags (type stripping).
const NODE_MIN: (u32, u32) = (22, 18);
/// The lowest Bun and Deno Haunt's scanner is meant for; both run TypeScript as it is.
const BUN_MIN: (u32, u32) = (1, 1);
const DENO_MIN: (u32, u32) = (2, 0);
/// The lowest git with the flags Haunt relies on (`--end-of-options`).
const GIT_MIN: (u32, u32) = (2, 24);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKind {
    Node,
    Bun,
    Deno,
}

/// What runs the scanner (`sidecar/`): Node when it is new enough, else Bun, else Deno.
#[derive(Clone, Debug)]
pub struct Runtime {
    pub kind: RuntimeKind,
    pub path: PathBuf,
}

impl Runtime {
    /// The command that runs `script`, a file of the scanner. Deno is let do what Node and Bun do (read, write, run
    /// the project's architecture script), and finds Haunt's own TypeScript in `node_modules` as Node does, without a
    /// package.json or deno.json (the installed app has neither).
    pub fn command(&self, script: &Path) -> Command {
        let mut command = Command::new(&self.path);
        if self.kind == RuntimeKind::Deno {
            command.args(["run", "--allow-all", "--quiet", "--no-lock", "--no-config", "--node-modules-dir=manual"]);
        }
        command.arg(script);
        command
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            RuntimeKind::Node => "Node",
            RuntimeKind::Bun => "Bun",
            RuntimeKind::Deno => "Deno",
        }
    }
}

/// The runtime for the scanner: Node 22.18 or later, else Bun, else Deno; an older Node is passed over for either.
/// HAUNT_RUNTIME (`node`, `bun` or `deno`) asks for one only, to run Haunt's tests on each.
pub fn find_runtime() -> Result<Runtime, String> {
    let only = std::env::var("HAUNT_RUNTIME").ok();
    let mut old_node = None;
    for (kind, program, min) in [
        (RuntimeKind::Node, "node", NODE_MIN),
        (RuntimeKind::Bun, "bun", BUN_MIN),
        (RuntimeKind::Deno, "deno", DENO_MIN),
    ] {
        if only.as_deref().is_some_and(|only| only != program) {
            continue;
        }
        let Some((path, version)) = find_program(program) else { continue };
        if major_minor(&version).is_some_and(|found| found >= min) {
            return Ok(Runtime { kind, path });
        }
        if kind == RuntimeKind::Node {
            old_node = Some(version);
        }
    }
    Err(match old_node {
        Some(version) => format!(
            "Haunt needs Node {}.{} or later to read TypeScript and JavaScript; this machine has {version}. PHP, Rust, Lua, Python and Go are still read. Update Node (or install Bun or Deno), then open Haunt again.",
            NODE_MIN.0, NODE_MIN.1
        ),
        None => "No JavaScript runtime was found, so Haunt reads only PHP, Rust, Lua, Python and Go. For TypeScript and JavaScript, install Node 22.18 or later from nodejs.org (or Bun or Deno), then open Haunt again.".into(),
    })
}

/// Node on its own, new enough to run a project's tests, which run on Node's own runner or the project's.
pub fn find_node() -> Result<PathBuf, String> {
    match find_program("node") {
        Some((path, version)) if major_minor(&version).is_some_and(|found| found >= NODE_MIN) => Ok(path),
        _ => Err(format!(
            "Running tests needs Node {}.{} or later: install it from nodejs.org, then open Haunt again.",
            NODE_MIN.0, NODE_MIN.1
        )),
    }
}

/// A program on the PATH, and the version it says; on macOS and Linux also through the login shell, since an app
/// started from the desktop does not get the PATH your shell sets up (Homebrew, nvm, fnm, Volta, ~/.bun, ~/.deno).
fn find_program(name: &str) -> Option<(PathBuf, String)> {
    match version_of(Path::new(name)) {
        Some(version) => Some((PathBuf::from(name), version)),
        None => from_login_shell(name),
    }
}

#[cfg(not(windows))]
fn from_login_shell(name: &str) -> Option<(PathBuf, String)> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".into()
        } else {
            "/bin/sh".into()
        }
    });
    let output =
        Command::new(shell).args(["-ilc", &format!("command -v {name}")]).stdin(Stdio::null()).output().ok()?;
    let found = String::from_utf8_lossy(&output.stdout);
    let path = PathBuf::from(found.lines().map(str::trim).filter(|line| line.starts_with('/')).last()?);
    let version = version_of(&path)?;
    Some((path, version))
}

#[cfg(windows)]
fn from_login_shell(_name: &str) -> Option<(PathBuf, String)> {
    None
}

fn version_of(program: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command.arg("--version").stdin(Stdio::null());
    hide_console(&mut command);
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// "v22.18.0" → (22, 18); "git version 2.47.1.windows.1" → (2, 47).
pub fn major_minor(text: &str) -> Option<(u32, u32)> {
    let digits = text.trim_start_matches(|c: char| !c.is_ascii_digit());
    let mut parts = digits
        .split('.')
        .map(|part| part.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok());
    Some((parts.next()??, parts.next()??))
}

pub fn check_git() -> Result<(), String> {
    match version_of(Path::new("git")) {
        None => Err("Git was not found. Haunt works with the git on your machine: install it from git-scm.com, then open Haunt again.".into()),
        Some(version) => match major_minor(&version) {
            Some(found) if found >= GIT_MIN => Ok(()),
            _ => Err(format!("Haunt needs git {}.{} or later; this machine has \"{version}\". Update it from git-scm.com, then open Haunt again.", GIT_MIN.0, GIT_MIN.1)),
        },
    }
}

#[cfg(windows)]
pub fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions() {
        assert_eq!(major_minor("v22.18.0"), Some((22, 18)));
        assert_eq!(major_minor("git version 2.47.1.windows.1"), Some((2, 47)));
        assert_eq!(major_minor("git version 2.9.0"), Some((2, 9)));
        assert!(major_minor("git version 2.9.0").unwrap() < GIT_MIN);
        assert_eq!(major_minor("nonsense"), None);
    }

    #[test]
    fn finds_this_machines_git() {
        assert!(check_git().is_ok());
    }

    #[test]
    fn runs_the_scanner_on_each_runtime() {
        let script = Path::new("app").join("sidecar").join("main.ts");
        let args = |kind| {
            let command = Runtime { kind, path: PathBuf::from("x") }.command(&script);
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>()
        };
        let main = script.to_string_lossy().into_owned();
        assert_eq!(args(RuntimeKind::Node), [main.as_str()]);
        assert_eq!(args(RuntimeKind::Bun), [main.as_str()]);
        let deno = args(RuntimeKind::Deno);
        assert_eq!(deno.first().map(String::as_str), Some("run"));
        assert!(deno.contains(&"--allow-all".to_owned()) && deno.contains(&"--node-modules-dir=manual".to_owned()));
        assert_eq!(deno.last(), Some(&main));
    }
}
