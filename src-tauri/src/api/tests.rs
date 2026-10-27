//! The window's commands end to end, and the bindings written from them.

use super::*;
use crate::events::Events;
use crate::repos::Context;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

/// The window's commands, end to end through `dispatch`: Rust starts the real Node sidecar on a fixture project.
#[test]
fn opens_a_project_scans_it_and_answers_for_it_only() {
    let base = crate::paths::canonical(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).unwrap();
    let fixture = format!("{base}{}sidecar{0}fixtures{0}single", std::path::MAIN_SEPARATOR);
    let settings = std::env::temp_dir().join(format!("haunt-api-{}.json", std::process::id()));
    let runtime = crate::startup::find_runtime();
    assert!(runtime.is_ok(), "the tests need Node, Bun or Deno");
    let context = Context {
        runtime,
        script: PathBuf::from(format!("{base}/sidecar/main.ts")),
        settings: settings.clone(),
        log: None,
        cache: None,
        logs: None,
    };
    let api = Api { repos: Repos::new(context, Arc::new(Events::default())), problems: vec![] };
    let call =
        |name: &str, args: Value| dispatch(&api, name, &args).map(|text| serde_json::from_str::<Value>(&text).unwrap());

    let info = call("open", json!({ "path": fixture })).unwrap();
    let repo = info["root"].as_str().unwrap().to_owned();
    assert_eq!(info["tabs"], json!([repo]));

    // The structure is null until the first scan is in.
    let mut structure = Value::Null;
    for _ in 0..300 {
        structure = call("structure", json!({ "repo": repo })).unwrap();
        if !structure.is_null() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let files: Vec<String> =
        structure["files"].as_array().unwrap().iter().map(|file| file["path"].as_str().unwrap().to_owned()).collect();
    assert!(files.iter().any(|path| path.ends_with(".ts")));

    // The call graph follows the structure.
    let mut calls = Value::Null;
    for _ in 0..100 {
        calls = call("calls", json!({ "repo": repo })).unwrap();
        if !calls.is_null() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(calls["scannedAt"], structure["scannedAt"]);

    // Git from Rust (the fixture sits in Haunt's own repository), a scanner request through the sidecar, and the
    // source of a scanned file; a file outside the scan is refused.
    assert!(call("refs", json!({ "repo": repo })).unwrap()["branches"].is_array());
    assert!(call("log", json!({ "repo": repo, "query": { "limit": 2 } })).unwrap()["commits"].is_array());
    assert!(call("changes", json!({ "repo": repo, "base": { "kind": "uncommitted" } })).is_ok());
    let source = call("source", json!({ "repo": repo, "path": files[0], "start": 1, "end": 2 })).unwrap();
    assert_eq!(source["start"], 1);
    assert!(call("source", json!({ "repo": repo, "path": "../../Cargo.toml", "start": 1, "end": 2 })).is_err());
    // Arguments come by their camelCase names, as Tauri sends them.
    assert!(call("open_file", json!({ "repo": repo, "editor": "nonsense", "path": files[0], "line": 1 }))
        .unwrap_err()
        .contains("does not know the editor"));

    // A call for a repository that is not shown is refused: an answer cannot land in the wrong tab.
    assert!(call("structure", json!({ "repo": base })).is_err());
    assert!(call("log", json!({ "repo": base, "query": {} })).is_err());
    assert!(call("nonsense", json!({})).is_err());

    // Closing its tab leaves nothing shown.
    let closed = call("close", json!({ "path": repo })).unwrap();
    assert!(closed["root"].is_null());
    let _ = std::fs::remove_file(settings);
}

/// PHP and Rust in a scan, end to end: the sidecar asks Rust for those files while it scans, and puts them into the
/// project's parts and the imports between parts.
#[test]
fn scans_php_and_rust_through_rust() {
    let base = crate::paths::canonical(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).unwrap();
    let settings = std::env::temp_dir().join(format!("haunt-api-others-{}.json", std::process::id()));
    let context = Context {
        runtime: crate::startup::find_runtime(),
        script: PathBuf::from(format!("{base}/sidecar/main.ts")),
        settings: settings.clone(),
        log: None,
        cache: None,
        logs: None,
    };
    let api = Api { repos: Repos::new(context, Arc::new(Events::default())), problems: vec![] };
    let call =
        |name: &str, args: Value| dispatch(&api, name, &args).map(|text| serde_json::from_str::<Value>(&text).unwrap());
    let scan = |fixture: &str| {
        let path = format!("{base}{}sidecar{0}fixtures{0}{fixture}", std::path::MAIN_SEPARATOR);
        let repo = call("open", json!({ "path": path })).unwrap()["root"].as_str().unwrap().to_owned();
        for _ in 0..300 {
            let structure = call("structure", json!({ "repo": repo })).unwrap();
            if !structure.is_null() {
                return structure;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("{fixture} was not scanned");
    };
    let imports = |structure: &Value| {
        let mut out: Vec<String> = structure["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| format!("{} → {}", group["from"].as_str().unwrap(), group["to"].as_str().unwrap()))
            .collect();
        out.sort();
        out
    };

    let laravel = scan("laravel");
    let first = api.repos.scanner();
    // Another version of a TypeScript file comes from the reader, while the sidecar scans.
    let repo = laravel["root"].as_str().unwrap().to_owned();
    let read = api.read_version(&repo, "resources/app.ts", "export function boot() {\n  return 1\n}\n");
    assert_eq!(read.iter().map(|function| function.name.as_str()).collect::<Vec<_>>(), ["boot"]);
    let mut parts: Vec<&str> =
        laravel["parts"].as_array().unwrap().iter().map(|part| part["name"].as_str().unwrap()).collect();
    parts.sort();
    assert_eq!(parts, ["Http", "Models", "Services", "app", "routes", "tests"]);
    let test =
        laravel["files"].as_array().unwrap().iter().find(|file| file["path"] == "tests/Feature/OrderTest.php").unwrap();
    assert_eq!(test["test"], true);
    // Tests stay out of the imports.
    assert_eq!(imports(&laravel), ["Http → Models", "Http → Services", "routes → Http"]);

    let workspace = scan("rust-workspace");
    assert_eq!(imports(&workspace), ["app-cli → app-core"]);

    // Back to the first tab: its scanner kept running, and is asked to scan again instead of started anew.
    scan("laravel");
    assert!(first.is_some());
    assert_eq!(api.repos.scanner(), first);
    let _ = std::fs::remove_file(settings);
}

/// Without a JavaScript runtime the scan is done in Rust (code/scan.rs): for the languages Rust reads it finds what
/// the sidecar finds (files, parts, imports and their breaks, cycles), and the window gets it the same way.
#[test]
fn scans_without_a_runtime_as_the_sidecar_does() {
    let base = crate::paths::canonical(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).unwrap();
    let fixture = |name: &str| format!("{base}{}sidecar{0}fixtures{0}{name}", std::path::MAIN_SEPARATOR);
    let repos = |runtime: Result<crate::startup::Runtime, String>, name: &str| {
        let settings = std::env::temp_dir().join(format!("haunt-api-{name}-{}.json", std::process::id()));
        let context = Context {
            runtime,
            script: PathBuf::from(format!("{base}/sidecar/main.ts")),
            settings,
            log: None,
            cache: None,
            logs: None,
        };
        Api { repos: Repos::new(context, Arc::new(Events::default())), problems: vec![] }
    };
    let wait = |api: &Api, name: &str, path: &str| {
        let call = |name: &str, args: Value| {
            dispatch(api, name, &args).map(|text| serde_json::from_str::<Value>(&text).unwrap())
        };
        let repo = call("open", json!({ "path": path })).unwrap()["root"].as_str().unwrap().to_owned();
        for _ in 0..300 {
            let structure = call("structure", json!({ "repo": repo })).unwrap();
            let calls = call("calls", json!({ "repo": repo })).unwrap();
            if !structure.is_null() && !calls.is_null() {
                return (repo, structure, calls);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("{name} was not scanned");
    };
    let typescript =
        |path: &str| crate::code::languages::language_of(path) == Some(crate::code::languages::Language::Typescript);
    let file_imports = |scan: &Value| {
        let mut out: Vec<Value> = scan["imports"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["files"].as_array().unwrap().clone())
            .filter(|file| !typescript(file["from"].as_str().unwrap()) && !typescript(file["to"].as_str().unwrap()))
            .collect();
        out.sort_by_key(|file| file.to_string());
        out
    };

    let with_node = repos(crate::startup::find_runtime(), "with");
    for name in [
        "laravel",
        "rust-workspace",
        "go-app",
        "python-app",
        "lua-app",
        "wp-plugin",
        "java-app",
        "csharp-app",
        "ruby-app",
        "c-app",
        "cpp-app",
    ] {
        let (repo, sidecar, _) = wait(&with_node, name, &fixture(name));
        let here: Value = serde_json::from_str(&crate::code::scan::scan(&repo, false).unwrap().structure).unwrap();
        let files: Vec<&Value> = sidecar["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|file| !typescript(file["path"].as_str().unwrap()))
            .collect();
        assert_eq!(here["files"].as_array().unwrap().iter().collect::<Vec<_>>(), files, "{name}: files");
        for part in here["parts"].as_array().unwrap() {
            assert!(sidecar["parts"].as_array().unwrap().contains(part), "{name}: part {part}");
        }
        assert_eq!(file_imports(&here), file_imports(&sidecar), "{name}: imports");
        assert_eq!(here["cycles"], sidecar["cycles"], "{name}: cycles");
        assert_eq!(here["project"], sidecar["project"], "{name}: project");
        for layer in here["layers"].as_array().unwrap() {
            assert!(sidecar["layers"].as_array().unwrap().contains(layer), "{name}: layer {layer}");
        }
    }

    // No runtime at all: the window gets a structure and its calls, and the scanner's questions are answered here.
    let without = repos(Err("No JavaScript runtime was found.".into()), "without");
    let (repo, structure, calls) = wait(&without, "python-app", &fixture("python-app"));
    assert!(!structure["files"].as_array().unwrap().is_empty());
    assert_eq!(calls["scannedAt"], structure["scannedAt"]);
    assert!(!calls["calls"].as_array().unwrap().is_empty());
    assert_eq!(without.repos.ask_scanner(&repo, json!({ "type": "routes" })).unwrap(), "[]");
    assert!(without.repos.scanner().is_none());
}

/// The bindings the window uses are written from these commands; this keeps `app/bindings.ts` current.
#[test]
fn writes_the_windows_bindings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../app/bindings.ts");
    crate::bindings()
        .export(
            specta_typescript::Typescript::default()
                .header("// Written by the Rust tests (src-tauri/src/api.rs) from Haunt's commands: do not edit.\n"),
            path,
        )
        .expect("the bindings could not be written");
    let written = std::fs::read_to_string(path).unwrap();
    for name in ["log", "commit", "openFile", "structure"] {
        assert!(written.contains(&format!("	{name}: (")), "{name} is missing from the bindings");
    }
}
