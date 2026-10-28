//! Running the tests that reach a change, with the project's own test runner: Vitest or Jest when the test file's
//! package uses one, node's own runner otherwise. Only in a trusted folder (tests are the project's own code), only
//! files inside the repository, one run at a time, and it can be stopped. Each file runs in its nearest package (where
//! its `package.json` is), and only the named tests, by a pattern made from their titles.

use parking_lot::Mutex;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// The output kept of a run: its end, where the summary is.
const KEEP: usize = 200_000;

static RUNNING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// A test to run: its file, relative to the repository, and its title.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct TestToRun {
    pub path: String,
    pub title: String,
}

/// How a run went: each package's command and output, in order.
#[derive(Debug, Clone, Serialize, Type)]
pub struct TestRun {
    /// Every package's run passed.
    pub passed: bool,
    /// Stopped from the window before it finished.
    pub stopped: bool,
    pub runs: Vec<PackageRun>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct PackageRun {
    /// The package's folder, relative to the repository ("" for its root).
    pub folder: String,
    /// The command, as it would be typed there.
    pub command: String,
    pub passed: bool,
    pub output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Vitest,
    Jest,
    Node,
}

/// Runs `tests` in `root` with `node`. Refused while another run is going.
pub fn run(root: &str, node: &Path, tests: &[TestToRun]) -> Result<TestRun, String> {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut running = RUNNING.lock();
        if running.is_some() {
            return Err(said!("testrun.running", "Tests are running already: wait for them, or stop them."));
        }
        *running = Some(stop.clone());
    }
    let result = run_groups(root, node, tests, &stop);
    *RUNNING.lock() = None;
    result
}

/// Stops the run going on, if there is one.
pub fn stop() {
    if let Some(stop) = RUNNING.lock().as_ref() {
        stop.store(true, Ordering::SeqCst);
    }
}

fn run_groups(root: &str, node: &Path, tests: &[TestToRun], stop: &AtomicBool) -> Result<TestRun, String> {
    let base = Path::new(root);
    // The tests by package, in the order first met.
    let mut groups: Vec<(PathBuf, Vec<&TestToRun>)> = Vec::new();
    for test in tests {
        let file = crate::paths::inside(root, &test.path)
            .ok_or_else(|| said!("testrun.outside", "{path} is not in this repository.", path = test.path))?;
        if !file.is_file() {
            return Err(said!("testrun.gone", "{path} is not there any more: scan again.", path = test.path));
        }
        let package = package_of(base, &file);
        match groups.iter_mut().find(|(folder, _)| *folder == package) {
            Some((_, items)) => items.push(test),
            None => groups.push((package, vec![test])),
        }
    }
    let mut runs = Vec::new();
    for (package, items) in groups {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let (runner, bin) = runner_of(base, &package);
        let mut files: Vec<String> = Vec::new();
        for item in &items {
            let file = relative(&package, &base.join(&item.path));
            if !files.contains(&file) {
                files.push(file);
            }
        }
        let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
        let pattern = pattern_of(&titles);
        let args = arguments(runner, bin.as_deref(), &files, &pattern);
        let folder = relative(base, &package);
        let command = format!("node {}", args.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join(" "));
        let (passed, output) = execute(node, &package, &args, stop)?;
        runs.push(PackageRun { folder, command, passed, output });
    }
    let stopped = stop.load(Ordering::SeqCst);
    Ok(TestRun { passed: !stopped && runs.iter().all(|run| run.passed), stopped, runs })
}

/// The nearest folder above `file`, up to `root`, with a `package.json`; `root` when there is none.
fn package_of(root: &Path, file: &Path) -> PathBuf {
    let mut at = file.parent();
    while let Some(folder) = at {
        if !folder.starts_with(root) {
            break;
        }
        if folder.join("package.json").is_file() {
            return folder.to_path_buf();
        }
        if folder == root {
            break;
        }
        at = folder.parent();
    }
    root.to_path_buf()
}

/// The package's runner, by its dependencies and its test script, and where that runner's command is (its package's
/// `bin`, in the nearest `node_modules` up to `root`). A runner Haunt cannot find falls back to node's own.
fn runner_of(root: &Path, package: &Path) -> (Runner, Option<PathBuf>) {
    let manifest = std::fs::read_to_string(package.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    let uses = |name: &str| {
        ["dependencies", "devDependencies"].iter().any(|key| manifest[key].get(name).is_some())
            || manifest["scripts"]["test"].as_str().is_some_and(|script| script.contains(name))
    };
    for (runner, name) in [(Runner::Vitest, "vitest"), (Runner::Jest, "jest")] {
        if uses(name) {
            if let Some(bin) = bin_of(root, package, name) {
                return (runner, Some(bin));
            }
        }
    }
    (Runner::Node, None)
}

fn bin_of(root: &Path, package: &Path, name: &str) -> Option<PathBuf> {
    let mut at = Some(package);
    while let Some(folder) = at {
        let dir = folder.join("node_modules").join(name);
        if let Some(manifest) = std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        {
            let bin = match &manifest["bin"] {
                Value::String(bin) => Some(bin.clone()),
                Value::Object(bins) => bins.get(name).and_then(Value::as_str).map(str::to_owned),
                _ => None,
            };
            return bin.map(|bin| dir.join(bin)).filter(|bin| bin.is_file());
        }
        if folder == root {
            break;
        }
        at = folder.parent().filter(|parent| parent.starts_with(root));
    }
    None
}

fn arguments(runner: Runner, bin: Option<&Path>, files: &[String], pattern: &str) -> Vec<String> {
    let bin = bin.map(|bin| bin.to_string_lossy().into_owned()).unwrap_or_default();
    let mut args = match runner {
        Runner::Vitest => vec![bin, "run".into(), "--testNamePattern".into(), pattern.into()],
        Runner::Jest => vec![bin, "--testNamePattern".into(), pattern.into()],
        Runner::Node => vec!["--test".into(), format!("--test-name-pattern={pattern}")],
    };
    if runner == Runner::Jest {
        args.push("--runTestsByPath".into());
    }
    args.extend(files.iter().cloned());
    args
}

/// One pattern for the titles: each one's text as written, with `${…}` and printf placeholders (`%s`, `%d`…) matching
/// anything, joined by `|`.
fn pattern_of(titles: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for title in titles {
        let mut out = String::new();
        let chars: Vec<char> = title.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            if c == '$' && chars.get(at + 1) == Some(&'{') {
                at = chars[at..].iter().position(|&c| c == '}').map_or(chars.len(), |end| at + end + 1);
                out.push_str(".*");
                continue;
            }
            if c == '%' && chars.get(at + 1).is_some_and(|next| "sdifjo#p".contains(*next)) {
                at += 2;
                out.push_str(".*");
                continue;
            }
            if "\\^$.|?*+()[]{}/".contains(c) {
                out.push('\\');
            }
            out.push(c);
            at += 1;
        }
        if !parts.contains(&out) {
            parts.push(out);
        }
    }
    parts.join("|")
}

fn execute(node: &Path, folder: &Path, args: &[String], stop: &AtomicBool) -> Result<(bool, String), String> {
    let mut command = crate::process::command(node);
    command
        .args(args)
        .current_dir(folder)
        .env("CI", "1")
        .env("FORCE_COLOR", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::startup::hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| said!("testrun.notStarted", "The tests could not start: {error}", error = error))?;
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let readers: Vec<_> = [
        child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|mut pipe| {
        let output = output.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = pipe.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                output.lock().extend_from_slice(&buffer[..read]);
            }
        })
    })
    .collect();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break Some(status);
        }
        if stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for reader in readers {
        let _ = reader.join();
    }
    let bytes = output.lock();
    let text = String::from_utf8_lossy(&bytes);
    let text = strip_ansi(&text);
    let kept = if text.len() > KEEP {
        let mut from = text.len() - KEEP;
        while !text.is_char_boundary(from) {
            from += 1;
        }
        format!("…\n{}", &text[from..])
    } else {
        text
    };
    Ok((status.is_some_and(|status| status.success()), kept))
}

/// The text without terminal colour codes (some runners colour even when asked not to).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn quote(arg: &str) -> String {
    if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=".contains(c)) {
        arg.to_owned()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_one_pattern_from_titles() {
        assert_eq!(pattern_of(&["adds two numbers", "adds two numbers"]), "adds two numbers");
        assert_eq!(pattern_of(&["a (b)", "row %s", "adds ${'more'} too"]), r"a \(b\)|row .*|adds .* too");
    }

    #[test]
    fn finds_the_package_and_its_runner() {
        let dir = std::env::temp_dir().join(format!("haunt-testrun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let package = dir.join("packages/lib");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/vitest")).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        std::fs::write(package.join("package.json"), r#"{ "devDependencies": { "vitest": "^3" } }"#).unwrap();
        std::fs::write(dir.join("node_modules/vitest/package.json"), r#"{ "bin": { "vitest": "./vitest.mjs" } }"#)
            .unwrap();
        std::fs::write(dir.join("node_modules/vitest/vitest.mjs"), "").unwrap();
        let file = package.join("src/a.test.ts");
        std::fs::write(&file, "").unwrap();

        assert_eq!(package_of(&dir, &file), package);
        let (runner, bin) = runner_of(&dir, &package);
        assert_eq!(runner, Runner::Vitest);
        assert_eq!(bin, Some(dir.join("node_modules/vitest").join("./vitest.mjs")));
        // The root uses no runner of its own: node's.
        assert_eq!(runner_of(&dir, &dir), (Runner::Node, None));
        let args = arguments(Runner::Node, None, &["a.test.ts".into()], "x|y");
        assert_eq!(args, vec!["--test", "--test-name-pattern=x|y", "a.test.ts"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn strips_colours() {
        assert_eq!(strip_ansi("\u{1b}[32m✓\u{1b}[39m ok"), "✓ ok");
    }
}
