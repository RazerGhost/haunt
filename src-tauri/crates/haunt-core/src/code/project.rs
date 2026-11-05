//! What resolving an import needs to know about the repository: its files, Composer's PSR-4 map for PHP, and its Rust
//! crates. Paths are relative to the repository with forward slashes, as everywhere in a scan.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

/// `path` with `.` and `..` resolved and repeated slashes dropped (as Node's `posix.normalize`); `..` stays in front.
pub fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." if out.last().is_some_and(|last| *last != "..") => {
                out.pop();
            }
            _ => out.push(segment),
        }
    }
    if out.is_empty() {
        ".".into()
    } else {
        out.join("/")
    }
}

pub fn join(base: &str, path: &str) -> String {
    normalize(&format!("{base}/{path}"))
}

/// The folder of `path`; `.` for a file at the top.
pub fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".into(),
        Some(at) => path[..at].into(),
        None => ".".into(),
    }
}

/// The first of `candidates` that is a file in the repository, normalised; none outside it.
pub fn file_in(root: &str, candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .map(|candidate| normalize(candidate))
        .find(|path| !path.starts_with("..") && Path::new(root).join(path).is_file())
}

/// PSR-4 prefixes to folders from composer.json's autoload (and autoload-dev), longest prefix first.
pub fn composer_psr4(root: &str) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(Path::new(root).join("composer.json")) else { return Vec::new() };
    let Ok(composer) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    let mut out = Vec::new();
    for section in ["autoload", "autoload-dev"] {
        let Some(map) = composer[section]["psr-4"].as_object() else { continue };
        for (prefix, folders) in map {
            let folders = match folders {
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                Value::String(folder) => vec![folder.as_str()],
                _ => Vec::new(),
            };
            for folder in folders {
                out.push((prefix.clone(), folder.trim_end_matches('/').to_owned()));
            }
        }
    }
    out.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
    out
}

/// The `name` under `[package]` in a Cargo.toml.
pub(super) fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = line.strip_prefix("name") else { continue };
        let Some(value) = rest.trim_start().strip_prefix('=') else { continue };
        let value = value.trim();
        if let Some(name) = value.strip_prefix('"').and_then(|value| value.split('"').next()) {
            return Some(name.to_owned());
        }
    }
    None
}

/// The repository's Rust crates: crate name (dashes as underscores) to its root module, from the Cargo.toml files given.
pub fn rust_crates(root: &str, manifests: &[String]) -> HashMap<String, String> {
    let mut crates = HashMap::new();
    for manifest in manifests {
        let Ok(text) = std::fs::read_to_string(Path::new(root).join(manifest)) else { continue };
        let folder = dirname(manifest);
        let file = file_in(root, &[join(&folder, "src/lib.rs"), join(&folder, "src/main.rs")]);
        if let (Some(name), Some(file)) = (package_name(&text), file) {
            crates.insert(name.replace('-', "_"), file);
        }
    }
    crates
}

/// The root module (src/lib.rs or src/main.rs) of the crate a Rust file belongs to.
pub fn rust_crate_root(root: &str, path: &str) -> Option<String> {
    let mut dir = dirname(path);
    loop {
        if Path::new(root).join(&dir).join("Cargo.toml").is_file() {
            return file_in(root, &[join(&dir, "src/lib.rs"), join(&dir, "src/main.rs")]);
        }
        if dir == "." || dir.is_empty() {
            return None;
        }
        dir = dirname(&dir);
    }
}

/// The Go module's path from go.mod (`module example.com/app`), to tell its own packages from others; empty without one.
pub fn go_module(root: &str) -> String {
    std::fs::read_to_string(Path::new(root).join("go.mod"))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.trim().strip_prefix("module ").map(|name| name.trim().trim_matches('"').to_owned())
            })
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_node_posix_has_them() {
        assert_eq!(normalize("a/./b/../c.php"), "a/c.php");
        assert_eq!(normalize("../x.php"), "../x.php");
        assert_eq!(normalize("a/../../x"), "../x");
        assert_eq!(join(".", "src/a.rs"), "src/a.rs");
        assert_eq!(dirname("src/a.rs"), "src");
        assert_eq!(dirname("a.rs"), ".");
        assert_eq!(
            package_name("[package]\nname = \"my-crate\"\n[dependencies]\nname = \"x\""),
            Some("my-crate".into())
        );
        assert_eq!(package_name("[workspace]\nmembers = []"), None);
    }
}
