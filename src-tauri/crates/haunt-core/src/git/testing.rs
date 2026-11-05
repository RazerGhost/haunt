//! A repository the tests build, in a folder of its own that goes when it is dropped: for core's tests and, through
//! the `test-support` feature, the app's.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Repo {
    _dir: tempfile::TempDir,
    pub root: String,
    /// Each commit a minute after the last, from yesterday: commits made in one second would tie in blame and log.
    clock: Cell<u64>,
}

// A new repository on disk each time, not a value to default to.
#[allow(clippy::new_without_default)]
impl Repo {
    pub fn new() -> Repo {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::plain(fs::canonicalize(dir.path()).unwrap()).to_string_lossy().into_owned();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let repo = Repo { _dir: dir, root, clock: Cell::new(now - 86_400) };
        repo.sh(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("core.autocrlf", "false"),
            ("commit.gpgsign", "false"),
        ] {
            repo.sh(&["config", key, value]);
        }
        repo
    }

    pub fn sh(&self, args: &[&str]) -> String {
        self.clock.set(self.clock.get() + 60);
        let date = format!("@{} +0000", self.clock.get());
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub fn write(&self, path: &str, text: &str) {
        fs::write(Path::new(&self.root).join(path), text).unwrap();
    }

    pub fn path(&self, path: &str) -> PathBuf {
        Path::new(&self.root).join(path)
    }
}
