//! How Haunt starts git: every run's environment and safety settings in one place, so they can be read together.
//!
//! - A read (`read`, behind `cli.rs`) runs nothing of the repository's own, trusted or not. Its config could make git
//!   run commands (a diff driver, a textconv, a clean or smudge filter, fsmonitor, a pager): fsmonitor, the untracked
//!   cache, `diff.external`, the pager and colour are switched off with `-c`, each filter the config names is made a
//!   plain `cat`, and the commands that would run a diff driver or textconv get `--no-ext-diff`/`--no-textconv`.
//!   Optional locks are off (`GIT_OPTIONAL_LOCKS=0`), so a read never takes the index lock from a git you are running
//!   yourself. Your own git config (system and global) stays: it holds core.autocrlf on Windows.
//! - A write (`write`, behind `Writer::git`) in a trusted folder runs git as a terminal would: hooks, filters, signing.
//!   In an untrusted one it runs none of its own: hooks point at an empty folder, fsmonitor and signing are off and
//!   the `ext::` transport is refused. No editor opens (`GIT_EDITOR=:`, `GIT_SEQUENCE_EDITOR=:`).
//! - A network run (`network`, behind `Writer::network`) is a write that may ask: git's questions go to the window
//!   through Haunt's askpass, or, run quietly, nothing asks (Git Credential Manager included). It runs in a process
//!   group of its own, so stopping it stops what it started (ssh, a credential helper). What an untrusted fetch adds
//!   on top (`network.rs`, `untrusted_fetch`) comes in as `extra` `-c` settings.
//! - Every run: no terminal prompt (`GIT_TERMINAL_PROMPT=0`), git's messages in English (`LC_ALL=C`, which Haunt reads),
//!   and no console window on Windows.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The `-c` settings of every read.
const READ_CONFIG: [&str; 12] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
    "-c",
    "diff.external=",
    "-c",
    "core.pager=cat",
    "-c",
    "color.ui=false",
    "-c",
    "log.showSignature=false",
];

/// Flags each read needs so that no diff driver or textconv command of the repository runs; added here, so no caller
/// can forget them (`git blame` runs a textconv command unless told not to).
fn safe_flags(command: &str) -> &'static [&'static str] {
    match command {
        "diff" | "diff-tree" | "log" | "show" => &["--no-ext-diff", "--no-textconv"],
        "blame" => &["--no-textconv"],
        _ => &[],
    }
}

/// A read of git `command` with `rest` in `root`: the repository's `filters` (the names its config gives) and
/// everything else that could run a command of its own off, optional locks, prompts and pagers off.
pub(crate) fn read(root: &str, filters: &[String], command: &str, rest: &[&str]) -> Command {
    let mut args: Vec<String> = READ_CONFIG.iter().map(|part| part.to_string()).collect();
    for name in filters {
        for setting in [
            format!("filter.{name}.clean=cat"),
            format!("filter.{name}.smudge=cat"),
            format!("filter.{name}.process="),
            format!("filter.{name}.required=false"),
        ] {
            args.push("-c".into());
            args.push(setting);
        }
    }
    args.push(command.to_owned());
    args.extend(safe_flags(command).iter().filter(|flag| !rest.contains(flag)).map(|flag| flag.to_string()));
    args.extend(rest.iter().map(|part| part.to_string()));
    let mut git = Command::new("git");
    git.args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    crate::startup::hide_console(&mut git);
    git
}

/// An empty folder for `core.hooksPath` in an untrusted folder: no hook can run from it.
fn no_hooks() -> PathBuf {
    let dir = std::env::temp_dir().join("haunt-no-hooks");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A write's `-c` settings: pager and colour off, and in an untrusted folder its hooks, fsmonitor, signing and the
/// `ext::` transport too.
fn write_config(trusted: bool) -> Vec<String> {
    let mut out: Vec<String> = ["color.ui=false", "core.pager=cat"].iter().map(|setting| setting.to_string()).collect();
    if !trusted {
        out.push(format!("core.hooksPath={}", no_hooks().to_string_lossy().replace('\\', "/")));
        out.extend(
            ["core.fsmonitor=false", "commit.gpgsign=false", "tag.gpgsign=false", "protocol.ext.allow=never"]
                .iter()
                .map(|setting| setting.to_string()),
        );
    }
    out.into_iter().flat_map(|setting| ["-c".to_string(), setting]).collect()
}

/// A writing git in `cwd`, its own code run only when `trusted`; the command's arguments come after.
pub(crate) fn write(cwd: &str, trusted: bool) -> Command {
    let mut git = Command::new("git");
    git.args(write_config(trusted))
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", ":")
        .env("GIT_SEQUENCE_EDITOR", ":")
        .env("LC_ALL", "C");
    crate::startup::hide_console(&mut git);
    git
}

/// A network run of git in `cwd`: a write with the untrusted fetch's `extra` settings, whose questions go to the
/// window for the tab `asks_for` ("" for none in particular), or nowhere when None (run quietly). In a process group
/// of its own; the command's arguments come after.
pub(crate) fn network(cwd: &str, trusted: bool, extra: &[String], asks_for: Option<&str>) -> Command {
    let mut git = write(cwd, trusted);
    git.args(extra.iter().flat_map(|setting| ["-c".to_string(), setting.clone()]))
        .envs(asks_for.map(crate::askpass::env).unwrap_or_default())
        .envs(asks_for.is_none().then_some(("GCM_INTERACTIVE", "never")));
    crate::process::own_group(&mut git);
    git
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    fn envs(command: &Command) -> Vec<(String, String)> {
        command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|v| v.to_string_lossy().into_owned()).unwrap_or_default(),
                )
            })
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        out.sort();
        out
    }

    fn sorted(mut list: Vec<(String, String)>) -> Vec<(String, String)> {
        list.sort();
        list
    }

    /// Each kind of run starts git exactly as it did before there was one place for it.
    #[test]
    fn starts_each_kind_of_git_as_before() {
        let read = read("/r", &["lfs".into()], "blame", &["-p", "--", "a.rs"]);
        assert_eq!(read.get_program(), "git");
        assert_eq!(read.get_current_dir(), Some(std::path::Path::new("/r")));
        assert_eq!(
            args(&read),
            [
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "diff.external=",
                "-c",
                "core.pager=cat",
                "-c",
                "color.ui=false",
                "-c",
                "log.showSignature=false",
                "-c",
                "filter.lfs.clean=cat",
                "-c",
                "filter.lfs.smudge=cat",
                "-c",
                "filter.lfs.process=",
                "-c",
                "filter.lfs.required=false",
                "blame",
                "--no-textconv",
                "-p",
                "--",
                "a.rs",
            ]
        );
        assert_eq!(
            sorted(envs(&read)),
            pairs(&[
                ("GIT_OPTIONAL_LOCKS", "0"),
                ("GIT_TERMINAL_PROMPT", "0"),
                ("GIT_PAGER", "cat"),
                ("PAGER", "cat"),
                ("LC_ALL", "C")
            ])
        );
        // A flag the caller gives is not given twice; a command without flags of its own gets none.
        assert_eq!(
            args(&super::read("/r", &[], "diff", &["--no-textconv"]))[12..],
            ["diff", "--no-ext-diff", "--no-textconv"]
        );
        assert_eq!(
            args(&super::read("/r", &[], "config", &["--get-regexp", "x"]))[12..],
            ["config", "--get-regexp", "x"]
        );

        let write_env =
            pairs(&[("GIT_TERMINAL_PROMPT", "0"), ("GIT_EDITOR", ":"), ("GIT_SEQUENCE_EDITOR", ":"), ("LC_ALL", "C")]);
        let trusted = write("/r", true);
        assert_eq!(args(&trusted), ["-c", "color.ui=false", "-c", "core.pager=cat"]);
        assert_eq!(sorted(envs(&trusted)), write_env);
        let hooks = format!("core.hooksPath={}", no_hooks().to_string_lossy().replace('\\', "/"));
        let untrusted_args = [
            "-c",
            "color.ui=false",
            "-c",
            "core.pager=cat",
            "-c",
            hooks.as_str(),
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "protocol.ext.allow=never",
        ];
        let untrusted = write("/r", false);
        assert_eq!(args(&untrusted), untrusted_args);
        assert_eq!(sorted(envs(&untrusted)), write_env);

        // Asking adds askpass's environment (none unless another test started it); quiet tells Git Credential Manager
        // not to ask.
        let extra = vec!["gc.auto=0".to_string(), "credential.helper=".to_string()];
        let asked = network("/r", false, &extra, Some("/r"));
        let mut with_extra: Vec<&str> = untrusted_args.to_vec();
        with_extra.extend(["-c", "gc.auto=0", "-c", "credential.helper="]);
        assert_eq!(args(&asked), with_extra);
        let mut asked_env = write_env.clone();
        asked_env.extend(crate::askpass::env("/r"));
        assert_eq!(sorted(envs(&asked)), sorted(asked_env));
        let quiet = network("/r", true, &[], None);
        assert_eq!(args(&quiet), ["-c", "color.ui=false", "-c", "core.pager=cat"]);
        let mut quiet_env = write_env.clone();
        quiet_env.push(("GCM_INTERACTIVE".into(), "never".into()));
        assert_eq!(sorted(envs(&quiet)), sorted(quiet_env));
    }
}
