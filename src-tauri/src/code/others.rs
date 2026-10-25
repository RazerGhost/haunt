//! The PHP, Rust, Lua, Python and Go files of a scan, read here for the sidecar: it puts the scan together (the project decides each
//! file's part and whether it is a test, and judges the imports), and asks for these files' functions, imports and
//! guessed calls while it scans. Readings are kept by modification time, so a rescan only reads what changed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::guess::guessed_calls;
use super::languages::{language_of, Language, MAX_SOURCE_BYTES};
use super::project::{composer_psr4, go_module, rust_crates};
use super::read::{read_file, Context, FileReading};
use super::types::{CallEdge, FileImport, FunctionNode};

/// The sidecar's request: the files to read and the repository's Cargo.toml files.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub files: Vec<Wanted>,
    pub cargo_manifests: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Wanted {
    pub path: String,
    /// A test's imports do not count when calls are matched.
    pub test: bool,
}

#[derive(Debug, Serialize)]
pub struct Read {
    pub tree: Vec<FunctionNode>,
    pub imports: Vec<FileImport>,
    pub lines: u32,
}

#[derive(Debug, Serialize)]
pub struct Answer {
    /// By path; a file that could not be read is left out.
    pub files: HashMap<String, Read>,
    pub calls: Vec<CallEdge>,
}

struct Kept {
    modified: SystemTime,
    reading: Arc<FileReading>,
    lines: u32,
}

/// Readings by repository, then by path.
fn kept() -> &'static Mutex<HashMap<String, HashMap<String, Kept>>> {
    static KEPT: OnceLock<Mutex<HashMap<String, HashMap<String, Kept>>>> = OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// Reads the files the sidecar asks for, from the readings kept where a file has not changed.
pub fn read(root: &str, request: &Request) -> Answer {
    let psr4 = composer_psr4(root);
    let crates = rust_crates(root, &request.cargo_manifests);
    let go = go_module(root);
    let mut readings: Vec<(String, Arc<FileReading>, u32, bool)> = Vec::new();
    for wanted in &request.files {
        let Some(language) = language_of(&wanted.path).filter(|language| *language != Language::Typescript) else {
            continue;
        };
        let file = Path::new(root).join(&wanted.path);
        let Ok(meta) = std::fs::metadata(&file) else { continue };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let hit = kept()
            .lock()
            .unwrap()
            .get(root)
            .and_then(|files| files.get(&wanted.path))
            .and_then(|kept| (kept.modified == modified).then(|| (kept.reading.clone(), kept.lines)));
        let (reading, lines) = match hit {
            Some(hit) => hit,
            None => {
                if meta.len() > MAX_SOURCE_BYTES {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&file) else { continue };
                let ctx = Context { root, path: &wanted.path, psr4: &psr4, crates: &crates, go_module: &go };
                let reading = Arc::new(read_file(language, &ctx, &source));
                let lines = source.split('\n').count() as u32;
                kept()
                    .lock()
                    .unwrap()
                    .entry(root.to_owned())
                    .or_default()
                    .insert(wanted.path.clone(), Kept { modified, reading: reading.clone(), lines });
                (reading, lines)
            }
        };
        readings.push((wanted.path.clone(), reading, lines, wanted.test));
    }
    let imports: Vec<(String, String)> = readings
        .iter()
        .filter(|(_, _, _, test)| !test)
        .flat_map(|(path, reading, _, _)| reading.imports.iter().map(move |to| (path.clone(), to.to.clone())))
        .collect();
    let by_path: Vec<(String, &FileReading)> =
        readings.iter().map(|(path, reading, _, _)| (path.clone(), &**reading)).collect();
    let calls = guessed_calls(&by_path, &imports);
    let files = readings
        .iter()
        .map(|(path, reading, lines, _)| {
            (path.clone(), Read { tree: reading.tree.clone(), imports: reading.imports.clone(), lines: *lines })
        })
        .collect();
    Answer { files, calls }
}

/// The functions of another version of a PHP, Rust, Lua, Python or Go file (its text at a commit), for the changes by function.
pub fn functions_of(root: &str, path: &str, source: &str) -> Vec<FunctionNode> {
    let Some(language) = language_of(path).filter(|language| *language != Language::Typescript) else {
        return Vec::new();
    };
    let crates = HashMap::new();
    read_file(language, &Context { root, path, psr4: &[], crates: &crates, go_module: "" }, source).tree
}

#[derive(Deserialize)]
struct Asked {
    #[serde(rename = "type")]
    kind: String,
    #[serde(flatten)]
    rest: serde_json::Value,
}

#[derive(Deserialize)]
struct Text {
    path: String,
    text: String,
}

/// Answers what the sidecar asks while it scans (a whole protocol line in, the answer's value as JSON text out):
/// `read-others` for the PHP, Rust, Lua, Python and Go files of a scan, `functions` for one such file's text at a commit.
pub fn answer(root: &str, line: &str) -> Result<String, String> {
    let asked: Asked = serde_json::from_str(line).map_err(|error| error.to_string())?;
    let value = match asked.kind.as_str() {
        "read-others" => {
            let request: Request = serde_json::from_value(asked.rest).map_err(|error| error.to_string())?;
            serde_json::to_string(&read(root, &request))
        }
        "functions" => {
            let text: Text = serde_json::from_value(asked.rest).map_err(|error| error.to_string())?;
            serde_json::to_string(&functions_of(root, &text.path, &text.text))
        }
        other => return Err(format!("Haunt does not know \"{other}\".")),
    };
    value.map_err(|error| error.to_string())
}

/// Forgets a repository's readings, when its tab closes.
pub fn forget(root: &str) {
    kept().lock().unwrap().remove(root);
}
