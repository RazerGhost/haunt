//! The scan done here, without a JavaScript runtime: when neither Node, Bun nor Deno is found, the PHP, Rust, Lua,
//! Python, Go, Java, C#, Ruby, C and C++ files are still scanned (their functions, imports and calls matched by name, read by `others.rs`),
//! and put together as the sidecar puts a scan together: the files to read and their parts (sidecar/project.ts and
//! projects/typescript.ts), workspaces (workspaces.ts), the project's haunt.config.json (config.ts), and the imports
//! between parts and their cycles (imports.ts). The TypeScript family is left out: it needs the TypeScript compiler.
//! A project's architecture script is code, so it is not run either.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::languages::{language_of, Language, MAX_SOURCE_BYTES};
use super::others::{self, Request, Wanted};
use super::project::{dirname, package_name};
use crate::sidecar::{Shape, ShapeFile};

/// Folders never searched: dependencies, builds and caches (workspaces.ts `IGNORED_FOLDERS`).
const IGNORED: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "out",
    "coverage",
    ".nuxt",
    ".output",
    ".next",
    ".svelte-kit",
    ".astro",
    ".turbo",
    ".cache",
    ".data",
    "target",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "obj",
];

/// A scan: the structure and the call graph as the sidecar sends them (JSON text), and the structure's shape.
pub struct Scanned {
    pub structure: String,
    pub calls: String,
    pub shape: Shape,
}

/// Scans `root`; None when it has no file of a language read here.
pub fn scan(root: &str, trusted: bool) -> Option<Scanned> {
    let started = Instant::now();
    let files = sources(root);
    if files.is_empty() {
        return None;
    }
    let packages = workspace_packages(root);
    let (config, architecture_error) = match read_config(root) {
        Ok(config) => (config, None),
        Err(error) => (None, Some(error)),
    };
    let script = config.as_ref().and_then(|config| config.script.clone());
    // The rules in haunt.config.json; a script's rules need a runtime to run.
    let rules = config.filter(|config| config.script.is_none());
    let architecture_error = architecture_error.or_else(|| {
        (trusted && script.is_some()).then(|| {
            "The architecture script runs on Node, Bun or Deno, and none was found: its layers are left out.".to_owned()
        })
    });
    let restart_on: Vec<String> = script.iter().map(|script| dirname(script)).filter(|folder| folder != ".").collect();

    // Where each file is: the config's rules first, then the zero-config parts.
    let mut located: HashMap<String, Located> = HashMap::new();
    let mut described: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for path in &files {
        let hit = rules.as_ref().and_then(|rules| rules.locate(path));
        if let Some(hit) = &hit {
            described.entry(hit.part.clone()).or_insert_with(|| (hit.role.clone(), hit.uses.clone()));
        }
        let found = match hit {
            Some(hit) => Located { part: hit.part, folder: hit.folder, layer: hit.layer },
            None => zero_config(&packages, path),
        };
        located.insert(path.clone(), found);
    }
    // An import may land on a file the scan did not list (in an ignored folder): it is placed the same way.
    let part_of = |path: &str| -> String {
        match located.get(path) {
            Some(found) => found.part.clone(),
            None => match rules.as_ref().and_then(|rules| rules.locate(path)) {
                Some(hit) => hit.part,
                None => zero_config(&packages, path).part,
            },
        }
    };

    let mut parts: Vec<Value> = Vec::new();
    let mut named: HashSet<&str> = HashSet::new();
    for path in &files {
        let found = &located[path];
        if named.insert(found.part.as_str()) {
            let (role, uses) = described.get(&found.part).cloned().unwrap_or_default();
            parts.push(
                json!({ "name": found.part, "layer": found.layer, "folder": found.folder, "role": role, "uses": uses }),
            );
        }
    }
    let ruled: Vec<&LayerConfig> = rules.as_ref().map(|rules| rules.layers.iter().collect()).unwrap_or_default();
    let ruled_ids: HashSet<&str> = ruled.iter().map(|layer| layer.id.as_str()).collect();
    let mut bands: Vec<&str> =
        located.values().map(|found| found.layer.as_str()).filter(|id| !ruled_ids.contains(id)).collect();
    bands.sort_by_key(|id| (band_order(id), *id));
    bands.dedup();
    let used: HashSet<&str> = located.values().map(|found| found.layer.as_str()).collect();
    let layers: Vec<Value> = ruled
        .iter()
        .map(|layer| {
            json!({ "id": layer.id, "label": layer.label.clone().unwrap_or_else(|| layer.id.clone()), "note": layer.note.clone().unwrap_or_default(), "ruled": true })
        })
        .chain(bands.iter().map(|id| {
            json!({ "id": id, "label": id, "note": if *id == OTHER { "files outside the workspace packages" } else { "" } })
        }))
        .filter(|layer| layer["id"].as_str().is_some_and(|id| used.contains(id)))
        .collect();

    let wanted: Vec<Wanted> = files.iter().map(|path| Wanted { path: path.clone(), test: is_test(path) }).collect();
    let request = Request { files: wanted, cargo_manifests: cargo_manifests(root) };
    let read = others::read(root, &request);
    let wanted = request.files;

    let mut scanned: Vec<Value> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for file in &wanted {
        let Some(reading) = read.files.get(&file.path) else { continue };
        scanned.push(json!({ "path": file.path, "part": part_of(&file.path), "test": file.test, "lines": reading.lines, "functions": reading.tree }));
        if file.test {
            continue;
        }
        for import in &reading.imports {
            if import.to == file.path {
                continue;
            }
            // A file imported for its values somewhere is a runtime import, however often it is imported for types.
            match seen.get(&(file.path.clone(), import.to.clone())) {
                Some(&at) => edges[at].type_only &= import.type_only,
                None => {
                    seen.insert((file.path.clone(), import.to.clone()), edges.len());
                    edges.push(Edge { from: file.path.clone(), to: import.to.clone(), type_only: import.type_only });
                }
            }
        }
    }
    let judge = |from: &str, to: &str| -> Vec<&'static str> {
        rules.as_ref().map(|rules| rules.judge(from, to)).unwrap_or_default()
    };
    let non_test: Vec<&str> = wanted.iter().filter(|file| !file.test).map(|file| file.path.as_str()).collect();
    let mut groups: Vec<(String, String, Vec<Value>)> = Vec::new();
    let mut inner: Vec<Value> = Vec::new();
    for edge in &edges {
        let (from, to) = (part_of(&edge.from), part_of(&edge.to));
        let (from, to) = (from.as_str(), to.as_str());
        if from == to {
            inner.push(json!({ "from": edge.from, "to": edge.to, "typeOnly": edge.type_only }));
            continue;
        }
        let file = json!({ "from": edge.from, "to": edge.to, "typeOnly": edge.type_only, "breaks": judge(&edge.from, &edge.to), "known": false });
        match groups.iter_mut().find(|(a, b, _)| a == from && b == to) {
            Some((_, _, files)) => files.push(file),
            None => groups.push((from.to_owned(), to.to_owned(), vec![file])),
        }
    }
    let imports: Vec<Value> =
        groups.into_iter().map(|(from, to, files)| json!({ "from": from, "to": to, "files": files })).collect();
    // In the sidecar's order (`localeCompare`): letters before their case, lowercase first.
    scanned.sort_by_cached_key(|file| {
        let path = file["path"].as_str().unwrap_or_default();
        (path.to_lowercase(), path.chars().map(|c| if c.is_uppercase() { 1 } else { 0 }).collect::<Vec<u8>>())
    });

    let mut watch: Vec<String> =
        files.iter().filter_map(|path| path.split_once('/').map(|(top, _)| top.to_owned())).collect();
    watch.extend(restart_on.iter().cloned());
    watch.sort();
    watch.dedup();
    let scanned_at = now_iso();
    let structure = json!({
        "root": root,
        "project": { "kind": "typescript", "name": project_name(root) },
        "watch": watch,
        "restartOn": restart_on,
        "trusted": trusted,
        "scannedAt": scanned_at,
        "ms": { "structure": started.elapsed().as_millis() as u64, "calls": null },
        "layers": layers,
        "parts": parts,
        "files": scanned,
        "calls": null,
        "imports": imports,
        "innerImports": inner,
        "cycles": runtime_cycles(&non_test, &edges),
        "rules": (rules.is_some() && !described.is_empty()).then_some("config"),
        "architectureError": architecture_error,
    });
    let shape = Shape {
        scanned_at: scanned_at.clone(),
        watch,
        restart_on,
        files: scanned
            .iter()
            .filter_map(|file| file["path"].as_str())
            .map(|path| ShapeFile { path: path.to_owned() })
            .collect(),
    };
    let calls = json!({ "event": "calls", "scannedAt": scanned_at, "calls": read.calls, "uses": {}, "ms": 0 });
    Some(Scanned { structure: structure.to_string(), calls: calls.to_string(), shape })
}

/// What the sidecar would answer after a structure (serve.ts), for a scan done here: no TypeScript routes, and no
/// imports read from other versions of files (the sidecar reads those only for the TypeScript family).
pub fn answer(request: &Value, structure: &str) -> Result<String, String> {
    match request["type"].as_str() {
        Some("routes") => Ok("[]".into()),
        Some("imports") => {
            let judged = serde_json::from_str::<Value>(structure).is_ok_and(|scan| scan["rules"] == "config");
            let files: serde_json::Map<String, Value> = request["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| file["key"].as_str())
                .map(|key| (key.to_owned(), json!([])))
                .collect();
            Ok(json!({ "judged": judged, "files": files }).to_string())
        }
        _ => Err("The scanner is not running.".into()),
    }
}

struct Edge {
    from: String,
    to: String,
    type_only: bool,
}

/// Every file of a language read here, outside dependencies, builds and hidden folders, small enough to have been
/// written by hand; relative, with forward slashes, sorted.
fn sources(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut folders = vec![String::new()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(Path::new(root).join(&folder)) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || IGNORED.contains(&name.as_str()) {
                continue;
            }
            let path = if folder.is_empty() { name } else { format!("{folder}/{name}") };
            // Followed through links, as the sidecar's walk does.
            let Ok(meta) = std::fs::metadata(entry.path()) else { continue };
            if meta.is_dir() {
                folders.push(path);
            } else if meta.len() <= MAX_SOURCE_BYTES
                && language_of(&path).is_some_and(|language| language != Language::Typescript)
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// A test file, as the sidecar tells them (languages.ts `isTestPath`): by its name, or a tests folder it is in.
fn is_test(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    name.ends_with("Test.php")
        || name.ends_with("_test.php")
        || name.ends_with("_spec.lua")
        || name.ends_with("_test.lua")
        || (name.ends_with(".py") && (name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py"))
        || name.ends_with("_test.go")
        || ["Test.java", "Tests.java", "Test.cs", "Tests.cs"].iter().any(|ending| name.ends_with(ending))
        || ["_spec.rb", "_test.rb", "_test.c", "_test.cc", "_test.cpp"].iter().any(|ending| name.ends_with(ending))
        || segments.iter().any(|folder| {
            matches!(
                *folder,
                "tests" | "test" | "testing" | "spec" | "__tests__" | "__mocks__" | "__fixtures__" | "fixtures" | "e2e"
            )
        })
}

/* Parts without configuration (projects/typescript.ts) */

const OTHER: &str = "other";

#[derive(Clone)]
struct Located {
    part: String,
    folder: String,
    layer: String,
}

fn band_order(id: &str) -> u8 {
    match id {
        "apps" => 0,
        OTHER => 2,
        _ => 1,
    }
}

/// Where a file belongs: its workspace package (the deepest), else a folder: in a workspace the top folder, otherwise
/// the folders in the source folder (src/, Laravel's app/, lib/), or at the root.
fn zero_config(packages: &[(String, String)], path: &str) -> Located {
    let owner = packages
        .iter()
        .filter(|(folder, _)| path.starts_with(&format!("{folder}/")))
        .max_by_key(|(folder, _)| folder.split('/').count());
    if let Some((folder, name)) = owner {
        let segments: Vec<&str> = folder.split('/').collect();
        let layer = if segments.len() > 1 { segments[0] } else { "packages" };
        return Located { part: name.clone(), folder: folder.clone(), layer: layer.into() };
    }
    let segments: Vec<&str> = path.split('/').collect();
    let root = || Located { part: "(root)".into(), folder: String::new(), layer: String::new() };
    if !packages.is_empty() {
        return if segments.len() > 1 {
            Located { part: segments[0].into(), folder: segments[0].into(), layer: OTHER.into() }
        } else {
            Located { layer: OTHER.into(), ..root() }
        };
    }
    if matches!(segments[0], "src" | "app" | "lib") && segments.len() > 2 {
        return Located {
            part: segments[1].into(),
            folder: format!("{}/{}", segments[0], segments[1]),
            layer: "parts".into(),
        };
    }
    if segments.len() > 1 {
        return Located { part: segments[0].into(), folder: segments[0].into(), layer: "parts".into() };
    }
    Located { layer: "parts".into(), ..root() }
}

/// The project's name: package.json's, composer.json's or the root crate's in Cargo.toml, else the folder's.
fn project_name(root: &str) -> String {
    for file in ["package.json", "composer.json"] {
        let name = read_json(&Path::new(root).join(file)).and_then(|json| json["name"].as_str().map(str::to_owned));
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            return name;
        }
    }
    std::fs::read_to_string(Path::new(root).join("Cargo.toml"))
        .ok()
        .and_then(|toml| package_name(&toml))
        .unwrap_or_else(|| {
            Path::new(root).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
        })
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/* Workspaces (workspaces.ts) */

/// The `packages:` list of a pnpm-workspace.yaml: enough YAML for a list of strings.
fn pnpm_patterns(yaml: &str) -> Vec<String> {
    let unquote = |text: &str| text.trim().trim_matches(|c| c == '"' || c == '\'').to_owned();
    let mut patterns = Vec::new();
    let mut in_packages = false;
    for line in yaml.lines() {
        if line.strip_prefix("packages").is_some_and(|rest| rest.trim_start().starts_with(':')) {
            in_packages = true;
            if let (Some(open), Some(close)) = (line.find('['), line.rfind(']')) {
                return line[open + 1..close].split(',').map(unquote).filter(|item| !item.is_empty()).collect();
            }
            continue;
        }
        if !in_packages {
            continue;
        }
        let item = line.trim_start();
        if line.starts_with(char::is_whitespace) && item.starts_with('-') {
            let value = item[1..].split('#').next().unwrap_or_default();
            let value = unquote(value);
            if !value.is_empty() {
                patterns.push(value);
            }
        } else if line.starts_with(|c: char| !c.is_whitespace()) {
            in_packages = false;
        }
    }
    patterns
}

fn workspace_patterns(root: &str) -> Vec<String> {
    if let Ok(yaml) = std::fs::read_to_string(Path::new(root).join("pnpm-workspace.yaml")) {
        return pnpm_patterns(&yaml);
    }
    let Some(package) = read_json(&Path::new(root).join("package.json")) else { return Vec::new() };
    let list = match &package["workspaces"] {
        Value::Array(items) => items,
        Value::Object(object) => match object.get("packages") {
            Some(Value::Array(items)) => items,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    list.iter().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// Folders up to five levels below the root that hold a `manifest` (package.json, Cargo.toml).
fn package_folders(root: &str, manifest: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut folders = vec![(String::new(), 1)];
    while let Some((folder, level)) = folders.pop() {
        if level > 5 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(Path::new(root).join(&folder)) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || IGNORED.contains(&name.as_str()) {
                continue;
            }
            if !std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            let child = if folder.is_empty() { name } else { format!("{folder}/{name}") };
            if Path::new(root).join(&child).join(manifest).exists() {
                found.push(child.clone());
            }
            folders.push((child, level + 1));
        }
    }
    found
}

/// The folders that `patterns` take (`!` patterns leave folders out), sorted.
fn matching(folders: Vec<String>, patterns: &[String]) -> Vec<String> {
    let (exclude, include): (Vec<&String>, Vec<&String>) =
        patterns.iter().partition(|pattern| pattern.starts_with('!'));
    let mut out: Vec<String> = folders
        .into_iter()
        .filter(|folder| {
            include.iter().any(|pattern| glob_match(pattern, folder).is_some())
                && !exclude.iter().any(|pattern| glob_match(&pattern[1..], folder).is_some())
        })
        .collect();
    out.sort();
    out
}

/// A TOML string list, enough of TOML for `members = ["crates/*", "app"]` under `[section]`.
fn toml_list(toml: &str, section: &str, key: &str) -> Vec<String> {
    let header = format!("[{section}]");
    let mut body = String::new();
    let mut inside = false;
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if inside {
                break;
            }
            inside = trimmed == header;
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    let is_key =
        |line: &&str| line.trim_start().strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='));
    let Some(start) = body.lines().position(|line| is_key(&line)) else { return Vec::new() };
    let rest = body.lines().skip(start).collect::<Vec<_>>().join("\n");
    let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) else { return Vec::new() };
    if close < open {
        return Vec::new();
    }
    // The quoted strings in the list.
    let mut list = &rest[open + 1..close];
    let mut out = Vec::new();
    while let Some(at) = list.find(['"', '\'']) {
        let quote = list[at..].chars().next().unwrap_or('"');
        let after = &list[at + 1..];
        let Some(end) = after.find(quote) else { break };
        out.push(after[..end].to_owned());
        list = &after[end + 1..];
    }
    out
}

fn cargo_name(root: &str, folder: &str) -> Option<String> {
    package_name(&std::fs::read_to_string(Path::new(root).join(folder).join("Cargo.toml")).ok()?)
}

/// A Cargo workspace's member crates, from `[workspace] members` (and `exclude`) in the root Cargo.toml.
fn cargo_members(root: &str) -> Vec<String> {
    let Ok(toml) = std::fs::read_to_string(Path::new(root).join("Cargo.toml")) else { return Vec::new() };
    let mut patterns = toml_list(&toml, "workspace", "members");
    if patterns.is_empty() {
        return Vec::new();
    }
    patterns.extend(toml_list(&toml, "workspace", "exclude").into_iter().map(|pattern| format!("!{pattern}")));
    matching(package_folders(root, "Cargo.toml"), &patterns)
}

/// Every Cargo.toml of the repository's crates: the root's when it is a crate itself, and the workspace members'.
fn cargo_manifests(root: &str) -> Vec<String> {
    let members = cargo_members(root).into_iter().map(|folder| format!("{folder}/Cargo.toml"));
    cargo_name(root, "").map(|_| "Cargo.toml".to_owned()).into_iter().chain(members).collect()
}

/// The workspace packages (folder, name), sorted by folder: JavaScript workspaces and Cargo workspaces' crates.
fn workspace_packages(root: &str) -> Vec<(String, String)> {
    let patterns = workspace_patterns(root);
    let mut packages: Vec<(String, String)> = if patterns.is_empty() {
        Vec::new()
    } else {
        matching(package_folders(root, "package.json"), &patterns)
            .into_iter()
            .map(|folder| {
                let name = read_json(&Path::new(root).join(&folder).join("package.json"))
                    .and_then(|json| json["name"].as_str().filter(|name| !name.is_empty()).map(str::to_owned));
                let name = name.unwrap_or_else(|| folder.clone());
                (folder, name)
            })
            .collect()
    };
    for folder in cargo_members(root) {
        if !packages.iter().any(|(known, _)| *known == folder) {
            let name = cargo_name(root, &folder).unwrap_or_else(|| folder.clone());
            packages.push((folder, name));
        }
    }
    packages.sort();
    packages
}

/// A folder glob matched against a folder: `*` is one folder's worth of characters, `**` any number of folders, `?`
/// one character. The text each `*` and `**` stood for, in order, or None when it does not match.
fn glob_match(pattern: &str, folder: &str) -> Option<Vec<String>> {
    let cleaned = pattern.strip_prefix("./").unwrap_or(pattern).trim_end_matches('/');
    let pattern: Vec<&str> = cleaned.split('/').collect();
    let folder: Vec<&str> = if folder.is_empty() { Vec::new() } else { folder.split('/').collect() };
    match_folders(&pattern, &folder)
}

fn match_folders(pattern: &[&str], folder: &[&str]) -> Option<Vec<String>> {
    let Some((first, rest)) = pattern.split_first() else { return folder.is_empty().then(Vec::new) };
    if *first == "**" {
        // As many folders as it can take, as a regular expression's `.*` would.
        return (0..=folder.len()).rev().find_map(|taken| {
            let mut more = match_folders(rest, &folder[taken..])?;
            more.insert(0, folder[..taken].join("/"));
            Some(more)
        });
    }
    let (name, others) = folder.split_first()?;
    let pattern: Vec<char> = first.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut found = match_name(&pattern, &name)?;
    found.extend(match_folders(rest, others)?);
    Some(found)
}

fn match_name(pattern: &[char], name: &[char]) -> Option<Vec<String>> {
    match pattern.split_first() {
        None => name.is_empty().then(Vec::new),
        Some(('*', rest)) => (0..=name.len()).rev().find_map(|taken| {
            let mut more = match_name(rest, &name[taken..])?;
            more.insert(0, name[..taken].iter().collect());
            Some(more)
        }),
        Some(('?', rest)) => match_name(rest, name.split_first()?.1),
        Some((c, rest)) => {
            let (first, others) = name.split_first()?;
            (first == c).then(|| match_name(rest, others)).flatten()
        }
    }
}

/* The project's haunt.config.json (config.ts) */

const CONFIG_FILE: &str = "haunt.config.json";

struct LayerConfig {
    id: String,
    label: Option<String>,
    note: Option<String>,
}

struct PartRule {
    pattern: String,
    name: Option<String>,
    layer: String,
    uses: Vec<String>,
    role: String,
}

struct Config {
    layers: Vec<LayerConfig>,
    parts: Vec<PartRule>,
    entries: Vec<String>,
    script: Option<String>,
}

struct ConfiguredPart {
    part: String,
    folder: String,
    layer: String,
    uses: Vec<String>,
    role: String,
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|text| !text.trim().is_empty()).map(str::to_owned)
}

fn text_list(value: &Value) -> Option<Vec<String>> {
    value.as_array()?.iter().map(text).collect()
}

/// The project's config, None when it has none; an error names what is wrong, in config.ts's words.
fn read_config(root: &str) -> Result<Option<Config>, String> {
    let Ok(raw) = std::fs::read_to_string(Path::new(root).join(CONFIG_FILE)) else { return Ok(None) };
    let raw: Value = serde_json::from_str(&raw).map_err(|error| format!("{CONFIG_FILE} is not valid JSON: {error}"))?;
    let Some(object) = raw.as_object() else { return Err(format!("{CONFIG_FILE} must hold an object.")) };
    let empty = Value::Array(Vec::new());
    let layers = object.get("layers").unwrap_or(&empty);
    let layers: Vec<LayerConfig> = layers
        .as_array()
        .and_then(|layers| {
            layers
                .iter()
                .map(|layer| {
                    Some(LayerConfig {
                        id: text(&layer["id"])?,
                        label: text(&layer["label"]),
                        note: text(&layer["note"]),
                    })
                })
                .collect()
        })
        .ok_or_else(|| format!("{CONFIG_FILE}: \"layers\" must be a list of {{ \"id\": \"…\" }}, top to bottom."))?;
    let ids: HashSet<&str> = layers.iter().map(|layer| layer.id.as_str()).collect();
    if ids.len() != layers.len() {
        return Err(format!("{CONFIG_FILE}: two layers have the same id."));
    }
    let Some(rules) = object.get("parts").unwrap_or(&empty).as_array() else {
        return Err(format!("{CONFIG_FILE}: \"parts\" must be a list of rules."));
    };
    let mut parts = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        let at = format!("{CONFIG_FILE}: parts[{index}]");
        let Some(pattern) = rule.as_object().and_then(|_| text(&rule["match"])) else {
            return Err(format!("{at} needs a \"match\" folder glob."));
        };
        let layer = text(&rule["layer"]).filter(|layer| ids.contains(layer.as_str()));
        let Some(layer) = layer else {
            let named = rule["layer"].as_str().unwrap_or("undefined");
            return Err(format!("{at} names layer \"{named}\", which \"layers\" does not list."));
        };
        let uses = match &rule["uses"] {
            Value::Null => Vec::new(),
            uses => text_list(uses).ok_or_else(|| format!("{at}: \"uses\" must be a list of part names."))?,
        };
        let name = match &rule["name"] {
            Value::Null => None,
            name => Some(text(name).ok_or_else(|| format!("{at}: \"name\" must be text."))?),
        };
        let role = rule["role"].as_str().unwrap_or_default().to_owned();
        parts.push(PartRule { pattern, name, layer, uses, role });
    }
    let entries = text_list(object.get("entries").unwrap_or(&empty))
        .ok_or_else(|| format!("{CONFIG_FILE}: \"entries\" must be a list of file names, e.g. [\"index.ts\"]."))?;
    let script = match object.get("architecture") {
        None => None,
        Some(architecture) => {
            let named = text(&architecture["script"]).ok_or_else(|| {
                format!("{CONFIG_FILE}: \"architecture\" must be {{ \"script\": \"path/to/rules.ts\" }}.")
            })?;
            let normalised = named.replace('\\', "/");
            let normalised = normalised.strip_prefix("./").unwrap_or(&normalised).to_owned();
            let drive = normalised.as_bytes().get(1) == Some(&b':');
            if normalised.starts_with('/') || drive || normalised.split('/').any(|segment| segment == "..") {
                return Err(format!("{CONFIG_FILE}: \"architecture.script\" must be a path inside the project."));
            }
            if ![".ts", ".mts", ".js", ".mjs"].iter().any(|end| normalised.ends_with(end)) {
                return Err(format!("{CONFIG_FILE}: \"architecture.script\" must be a .ts, .mts, .js or .mjs file."));
            }
            Some(normalised)
        }
    };
    Ok(Some(Config { layers, parts, entries, script }))
}

impl Config {
    /// Where a file belongs by the rules: the first rule that matches one of its folders, the shortest folder first.
    fn locate(&self, path: &str) -> Option<ConfiguredPart> {
        let folders: Vec<&str> = path.split('/').collect();
        let folders = &folders[..folders.len() - 1];
        for rule in &self.parts {
            for depth in 1..=folders.len() {
                let folder = folders[..depth].join("/");
                let Some(found) = glob_match(&rule.pattern, &folder) else { continue };
                let part = match &rule.name {
                    Some(name) => substitute(name, &folder, &found),
                    None => folders[depth - 1].to_owned(),
                };
                return Some(ConfiguredPart {
                    part,
                    folder,
                    layer: rule.layer.clone(),
                    uses: rule.uses.clone(),
                    role: rule.role.clone(),
                });
            }
        }
        None
    }

    /// What an import between two parts breaks: a layer above, or a part of the same layer the importing part does not
    /// list (`layer`); with `entries`, coming in past them (`door`). Files no rule placed are not judged.
    fn judge(&self, from: &str, to: &str) -> Vec<&'static str> {
        let (Some(a), Some(b)) = (self.locate(from), self.locate(to)) else { return Vec::new() };
        let rank = |id: &str| self.layers.iter().position(|layer| layer.id == id);
        let mut breaks = Vec::new();
        let lower = rank(&b.layer) > rank(&a.layer);
        if !(lower || (a.layer == b.layer && a.uses.contains(&b.part))) {
            breaks.push("layer");
        }
        if !self.entries.is_empty() && !self.entries.iter().any(|entry| to == format!("{}/{entry}", b.folder)) {
            breaks.push("door");
        }
        breaks
    }
}

/// A part's name with `$1`, `$2`… the glob's matches (`$0` the whole folder).
fn substitute(name: &str, folder: &str, found: &[String]) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().and_then(|next| next.to_digit(10))) {
            ('$', Some(group)) => {
                chars.next();
                out.push_str(if group == 0 {
                    folder
                } else {
                    found.get(group as usize - 1).map_or("", String::as_str)
                });
            }
            _ => out.push(c),
        }
    }
    out
}

/* Import cycles (imports.ts) */

/// The runtime import cycles (strongly connected components of two or more files), each sorted, the list sorted.
fn runtime_cycles(files: &[&str], edges: &[Edge]) -> Vec<Vec<String>> {
    let at: HashMap<&str, usize> = files.iter().enumerate().map(|(index, file)| (*file, index)).collect();
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); files.len()];
    for edge in edges.iter().filter(|edge| !edge.type_only) {
        if let (Some(&from), Some(&to)) = (at.get(edge.from.as_str()), at.get(edge.to.as_str())) {
            next[from].push(to);
        }
    }
    // Tarjan's algorithm, iterative so a deep graph cannot overflow the stack.
    let mut index: Vec<Option<usize>> = vec![None; files.len()];
    let mut low = vec![0; files.len()];
    let mut on_stack = vec![false; files.len()];
    let mut stack = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut counter = 0;
    for start in 0..files.len() {
        if index[start].is_some() {
            continue;
        }
        let mut work = vec![(start, 0)];
        index[start] = Some(counter);
        low[start] = counter;
        counter += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(&mut (node, ref mut edge)) = work.last_mut() {
            if let Some(&to) = next[node].get(*edge) {
                *edge += 1;
                match index[to] {
                    None => {
                        index[to] = Some(counter);
                        low[to] = counter;
                        counter += 1;
                        stack.push(to);
                        on_stack[to] = true;
                        work.push((to, 0));
                    }
                    Some(seen) if on_stack[to] => low[node] = low[node].min(seen),
                    Some(_) => {}
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if Some(low[node]) == index[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(files[member].to_owned());
                    if member == node {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort();
                    cycles.push(component);
                }
            }
        }
    }
    cycles.sort();
    cycles
}

/// Now as JavaScript's `toISOString()` writes it: 2026-10-25T12:00:00.000Z.
fn now_iso() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = now.as_secs();
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = (seconds / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let time = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600,
        time % 3_600 / 60,
        time % 60,
        now.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_folder_globs_as_the_sidecar_does() {
        assert_eq!(glob_match("packages/*", "packages/core"), Some(vec!["core".to_owned()]));
        assert_eq!(glob_match("packages/*", "packages/core/sub"), None);
        assert_eq!(glob_match("./crates/**/", "crates/a/b"), Some(vec!["a/b".to_owned()]));
        assert_eq!(glob_match("crates/**", "crates"), Some(vec![String::new()]));
        assert_eq!(glob_match("**/domain", "src/x/domain"), Some(vec!["src/x".to_owned()]));
        assert_eq!(glob_match("src/app-?", "src/app-1"), Some(Vec::new()));
        assert_eq!(glob_match("src/*-*", "src/a-b-c"), Some(vec!["a-b".to_owned(), "c".to_owned()]));
        assert_eq!(substitute("domain/$1", "src/domain/orders", &["orders".into()]), "domain/orders");
    }

    #[test]
    fn reads_workspace_lists() {
        assert_eq!(
            pnpm_patterns("packages:\n  - 'apps/*'\n  - packages/** # all\nother: 1\n  - no\n"),
            ["apps/*", "packages/**"]
        );
        assert_eq!(pnpm_patterns("packages: [\"a/*\", 'b']\n"), ["a/*", "b"]);
        let toml = "[package]\nname = \"x\"\n[workspace]\nmembers = [\n  \"crates/*\",\n  'app',\n]\nexclude = [\"crates/old\"]\n[dependencies]\n";
        assert_eq!(toml_list(toml, "workspace", "members"), ["crates/*", "app"]);
        assert_eq!(toml_list(toml, "workspace", "exclude"), ["crates/old"]);
        assert!(toml_list(toml, "dependencies", "members").is_empty());
    }

    #[test]
    fn tells_tests_by_name_and_folder() {
        assert!(is_test("tests/Feature/OrderTest.php"));
        assert!(is_test("app/test_orders.py") && is_test("conftest.py") && is_test("pkg/a_test.go"));
        assert!(is_test("spec/a.lua") && is_test("a_spec.lua"));
        assert!(is_test("src/OrderTest.java") && is_test("Shop.Tests/OrderTests.cs") && is_test("lib/order_spec.rb"));
        assert!(is_test("order_test.c") && is_test("order_test.cpp") && !is_test("src/Order.java"));
        assert!(!is_test("app/Models/Order.php") && !is_test("src/testing.py") && !is_test("contest.py"));
    }

    #[test]
    fn finds_import_cycles() {
        let edge = |from: &str, to: &str, type_only| Edge { from: from.into(), to: to.into(), type_only };
        let edges = [
            edge("a", "b", false),
            edge("b", "c", false),
            edge("c", "a", false),
            edge("c", "d", true),
            edge("d", "c", false),
        ];
        assert_eq!(runtime_cycles(&["a", "b", "c", "d"], &edges), [["a", "b", "c"]]);
    }

    /// haunt.config.json's rules in a scan done here: parts by rule, the rest by folder, and the imports judged.
    #[test]
    fn judges_imports_by_the_config() {
        let root = std::env::temp_dir().join(format!("haunt-scan-here-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |path: &str, text: &str| {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        write(
            "haunt.config.json",
            r#"{ "layers": [{ "id": "app" }, { "id": "domain", "label": "Domain" }],
                 "parts": [{ "match": "src/app", "layer": "app" }, { "match": "src/domain/*", "name": "domain/$1", "layer": "domain" }],
                 "entries": ["__init__.py"] }"#,
        );
        write("src/__init__.py", "");
        write("src/app/__init__.py", "");
        write("src/app/views.py", "from src.domain.orders.models import total\n\ndef show():\n    return total()\n");
        write("src/domain/__init__.py", "");
        write("src/domain/orders/__init__.py", "");
        write("src/domain/orders/models.py", "from src.app.views import show\n\ndef total():\n    return 1\n");
        write("tests/test_views.py", "from src.app.views import show\n");
        let root_text = root.to_string_lossy().into_owned();
        let scan: Value = serde_json::from_str(&scan(&root_text, false).unwrap().structure).unwrap();

        let parts: Vec<&str> =
            scan["parts"].as_array().unwrap().iter().map(|part| part["name"].as_str().unwrap()).collect();
        assert!(parts.contains(&"app") && parts.contains(&"domain/orders") && parts.contains(&"tests"), "{parts:?}");
        assert_eq!(scan["rules"], "config");
        let layers: Vec<&str> =
            scan["layers"].as_array().unwrap().iter().map(|layer| layer["id"].as_str().unwrap()).collect();
        assert_eq!(&layers[..2], ["app", "domain"]);
        let breaks = |from: &str| {
            scan["imports"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|group| group["files"].as_array().unwrap().clone())
                .find(|file| file["from"] == from)
                .map(|file| file["breaks"].clone())
        };
        // Down a layer, but past the door; up a layer and past the door.
        assert_eq!(breaks("src/app/views.py"), Some(json!(["door"])));
        assert_eq!(breaks("src/domain/orders/models.py"), Some(json!(["layer", "door"])));
        assert_eq!(scan["cycles"], json!([["src/app/views.py", "src/domain/orders/models.py"]]));
        // Tests are scanned, but their imports left out.
        assert!(breaks("tests/test_views.py").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn writes_the_time_as_javascript_does() {
        let now = now_iso();
        assert_eq!((now.len(), &now[4..5], &now[10..11], &now[23..]), (24, "-", "T", "Z"));
        assert!(now.as_str() > "2026-01-01");
    }
}
