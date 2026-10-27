//! Trust: whether a folder's own code may run, and what it would run (its TypeScript, its architecture script, its git
//! hooks), to read before trusting it.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use specta::Type;

use super::{git_dir, AppSettings, Repos, RootInfo};
use crate::paths;

/// Code of a folder that Haunt runs only once the folder is trusted.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Wanted {
    pub kind: WantedKind,
    /// What it is, for the trust banner.
    pub label: String,
    /// Its path, relative to the folder.
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WantedKind {
    Typescript,
    Script,
    /// A git hook: git runs it on a commit, push, checkout… that Haunt makes, in a trusted folder only.
    Hook,
}

/// What a folder would run, to read before trusting it: each file's text (a hook, a script), or what it is.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WantedText {
    pub label: String,
    pub path: String,
    /// Its text, at most `WANTED_TEXT` bytes; empty for what is not a file to read (the TypeScript compiler).
    pub text: String,
    /// What it is when there is no text to show, or what else to know.
    pub note: String,
    pub truncated: bool,
}

/// The most of a hook or script shown before trusting.
const WANTED_TEXT: usize = 64 * 1024;

/// The hooks git runs for what Haunt does: committing, pushing, checking out, merging, rebasing, applying patches.
const HOOKS: [&str; 14] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-merge-commit",
    "post-merge",
    "pre-push",
    "post-checkout",
    "pre-rebase",
    "post-rewrite",
    "reference-transaction",
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
];

impl Repos {
    /// Stops trusting a folder, shown or not. A tab of it that keeps its scanner running loses the scanner, which ran
    /// the folder's own code; the next time it is shown it is scanned as untrusted.
    pub fn forget_trust(&self, path: &str) -> AppSettings {
        let shown = self.inner.lock().shown.as_ref().is_some_and(|shown| shown.root == path);
        if shown {
            let _ = self.set_trust(path, false);
        } else {
            let mut inner = self.inner.lock();
            inner.settings.set_trust(path, false);
            inner.settings.save(&self.context.settings);
            for kept in inner.kept.iter_mut().filter(|kept| kept.root == path) {
                kept.scanner = None;
            }
        }
        self.app_settings()
    }

    /// Whether the shown repository's own code may run (its hooks and filters, for writing); refused for another one.
    pub fn trusted(&self, repo: &str) -> Result<bool, String> {
        let inner = self.inner.lock();
        self.check(&inner, repo)?;
        Ok(inner.settings.trusts(repo))
    }

    /// Lets the shown repository's own code run, or stops it; the scanner starts again either way.
    pub fn set_trust(&self, repo: &str, trusted: bool) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock();
            self.check(&inner, repo)?;
            inner.settings.set_trust(repo, trusted);
            inner.settings.save(&self.context.settings);
            self.restart(&mut inner, if trusted { "trusting this folder" } else { "no longer trusting this folder" });
        }
        self.events.send(Some(repo), json!({ "type": "trust", "trusted": trusted }));
        Ok(self.info())
    }
}

/// The folder git looks for hooks in (core.hooksPath, or the git folder's hooks), and the hooks there that git would run:
/// a file by a hook's name, executable where that counts.
fn hooks(root: &str) -> Vec<PathBuf> {
    let read = |args: &[&str]| {
        crate::git::cli::git(root, args).ok().map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
    };
    let folder = match read(&["config", "--get", "core.hooksPath"]) {
        Some(configured) => {
            let configured = PathBuf::from(configured);
            if configured.is_absolute() {
                configured
            } else {
                // Relative to the top of the working tree, as git takes it.
                read(&["rev-parse", "--show-toplevel"])
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(root))
                    .join(configured)
            }
        }
        None => match git_dir(root) {
            Some(dir) => dir.join("hooks"),
            None => return Vec::new(),
        },
    };
    HOOKS
        .iter()
        .map(|name| folder.join(name))
        .filter(|path| {
            let Ok(meta) = std::fs::metadata(path) else { return false };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && meta.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        })
        .collect()
}

/// A path for people: relative to the folder when inside it.
fn shown_path(root: &str, path: &Path) -> String {
    let plain = paths::plain(path.to_path_buf());
    let base = paths::plain(PathBuf::from(root));
    plain
        .strip_prefix(&base)
        .map(|rest| rest.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| plain.to_string_lossy().into_owned())
}

/// The code a folder would have Haunt run, found by looking at its files only: its own TypeScript, the architecture
/// script its haunt.config.json names, and its git hooks.
pub(super) fn wants(root: &str) -> Vec<Wanted> {
    let mut out = Vec::new();
    let base = Path::new(root);
    if base.join("node_modules/typescript/package.json").exists() {
        out.push(Wanted {
            kind: WantedKind::Typescript,
            label: said!("repos.ownTypescript", "own TypeScript"),
            path: "node_modules/typescript".into(),
        });
    }
    let script = std::fs::read_to_string(base.join("haunt.config.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|config| config["architecture"]["script"].as_str().map(str::to_owned));
    if let Some(script) = script {
        out.push(Wanted {
            kind: WantedKind::Script,
            label: said!("repos.architectureScript", "architecture script"),
            path: script,
        });
    }
    for hook in hooks(root) {
        let name = hook.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Wanted {
            kind: WantedKind::Hook,
            label: said!("repos.hook", "{name} hook", name = name),
            path: shown_path(root, &hook),
        });
    }
    out
}

/// What each thing a folder would run says, to read before trusting it. A hook that only hands over to Husky's file
/// (`.husky/<hook>`) shows that file too.
pub fn wanted_texts(root: &str) -> Vec<WantedText> {
    let base = Path::new(root);
    let read = |path: &Path| -> (String, bool) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let truncated = bytes.len() > WANTED_TEXT;
                (String::from_utf8_lossy(&bytes[..bytes.len().min(WANTED_TEXT)]).into_owned(), truncated)
            }
            Err(_) => (String::new(), false),
        }
    };
    let mut out = Vec::new();
    for want in wants(root) {
        let file = if Path::new(&want.path).is_absolute() { PathBuf::from(&want.path) } else { base.join(&want.path) };
        match want.kind {
            WantedKind::Typescript => {
                let version = std::fs::read_to_string(file.join("package.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|package| package["version"].as_str().map(str::to_owned));
                out.push(WantedText {
                    label: want.label,
                    path: want.path,
                    text: String::new(),
                    note: match version {
                        Some(version) => said!("repos.typescriptNote", "The TypeScript compiler installed in the folder ({version}), used to read its code with its own settings instead of Haunt's copy.", version = version),
                        None => said!("repos.typescriptNoteUnknown", "The TypeScript compiler installed in the folder (of an unknown version), used to read its code with its own settings instead of Haunt's copy."),
                    },
                    truncated: false,
                });
            }
            WantedKind::Script => {
                let (text, truncated) = read(&file);
                out.push(WantedText {
                    label: want.label,
                    path: want.path,
                    note: if text.is_empty() {
                        said!("repos.fileMissing", "The file is not there.")
                    } else {
                        said!("repos.scriptNote", "Run by the scanner with Node, to sort the code into layers.")
                    },
                    text,
                    truncated,
                });
            }
            WantedKind::Hook => {
                let (text, truncated) = read(&file);
                let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                out.push(WantedText {
                    label: want.label.clone(),
                    path: want.path.clone(),
                    note: said!("repos.hookNote", "Run by git when Haunt does what triggers it."),
                    text: text.clone(),
                    truncated,
                });
                let husky = base.join(".husky").join(&name);
                if text.contains("husky") && husky.is_file() && husky != file {
                    let (text, truncated) = read(&husky);
                    out.push(WantedText {
                        label: said!("repos.husky", "{name} (Husky)", name = name),
                        path: shown_path(root, &husky),
                        note: said!("repos.huskyNote", "What the hook above hands over to."),
                        text,
                        truncated,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn lists_the_hooks_git_would_run_with_their_text() {
        let r = crate::git::tests::Repo::new();
        let hooks = r.path(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        executable(&hooks.join("pre-commit"), "#!/bin/sh\nnpm test\n");
        executable(&hooks.join("pre-push.sample"), "#!/bin/sh\n");
        let found: Vec<(String, String)> =
            wants(&r.root).into_iter().map(|want| (crate::said::english(&want.label).to_owned(), want.path)).collect();
        assert_eq!(found, [("pre-commit hook".to_owned(), ".git/hooks/pre-commit".to_owned())], "samples do not run");
        let texts = wanted_texts(&r.root);
        assert!(texts[0].text.contains("npm test"));

        // Husky: core.hooksPath points at .husky/_, whose hook hands over to .husky/pre-push.
        std::fs::create_dir_all(r.path(".husky/_")).unwrap();
        executable(&r.path(".husky/_/pre-push"), "#!/usr/bin/env sh\n. \"$(dirname \"$0\")/husky.sh\"\n");
        std::fs::write(r.path(".husky/pre-push"), "pnpm lint\n").unwrap();
        r.sh(&["config", "core.hooksPath", ".husky/_"]);
        let texts = wanted_texts(&r.root);
        let labels: Vec<&str> = texts.iter().map(|text| crate::said::english(&text.label)).collect();
        assert_eq!(labels, ["pre-push hook", "pre-push (Husky)"]);
        assert_eq!(texts[0].path, ".husky/_/pre-push");
        assert!(texts[1].text.contains("pnpm lint"));
    }
}
