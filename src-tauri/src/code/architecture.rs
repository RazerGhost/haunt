//! What a change does to the architecture: the imports it adds and removes between files, each judged by the project's
//! rules (a layer broken, a part entered past its door), from the imports of every changed TypeScript-family file
//! before and after it. The repository's scanner reads them (`sidecar/scan.ts` `imports`), resolved and judged the way
//! the scan does, against the files as they are now: an import of a file that has since moved or gone is not seen.
//! And the same commit by commit along the current branch, for the breaks added and removed over time.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::languages::{language_of, Language};
use crate::git::diff::{self, FileStatus};

/// The most files a commit may change for its imports to be read in the history (each version of each is parsed).
const HISTORY_COMMIT_FILES: usize = 60;
/// The most commits the history reads.
const HISTORY_MOST: u32 = 200;
/// About how many file versions go to the scanner in one ask, for the history.
const HISTORY_ASK: usize = 300;

/// An import between two files, as the scanner judged it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub to: String,
    pub type_only: bool,
    pub from_part: String,
    pub to_part: String,
    /// The rules it breaks ("layer", "door"); empty when it breaks none or nothing judges imports.
    pub breaks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileImports {
    pub path: String,
    pub added: Vec<Import>,
    pub removed: Vec<Import>,
}

/// The imports a change adds and removes, by file.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ImportChanges {
    /// Whether the project's rules judge imports (a haunt.config.json, or a script with a judge).
    pub judged: bool,
    pub files: Vec<FileImports>,
}

/// One commit of the history: how many imports it added and removed, and how many of those break the rules.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitArchitecture {
    pub sha: String,
    pub subject: String,
    pub time: i64,
    pub added: u32,
    pub removed: u32,
    pub added_breaks: u32,
    pub removed_breaks: u32,
    /// Too big to read (more files than the history reads), or its files could not be read: no counts.
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ArchitectureHistory {
    pub judged: bool,
    /// Newest first, along the first parents of HEAD.
    pub commits: Vec<CommitArchitecture>,
}

/// What the scanner answers for a batch of versions.
#[derive(Deserialize)]
struct Read {
    judged: bool,
    files: HashMap<String, Vec<Import>>,
}

/// Asks the repository's scanner for the imports of file versions: `[{ key, path, text }]` in, its answer out.
pub type AskImports<'a> = dyn Fn(&[serde_json::Value]) -> Result<String, String> + 'a;

/// The imports `to` adds and removes compared with `from` (None: everything `to` has, its first commit).
pub fn changes(root: &str, from: Option<&str>, to: &str, ask: &AskImports) -> Result<ImportChanges, String> {
    let diffs: Vec<diff::FileDiff> = diff::commit_diff(root, from, to)?
        .into_iter()
        .filter(|diff| {
            !diff.binary
                && (language_of(&diff.path) == Some(Language::Typescript)
                    || language_of(&diff.old_path) == Some(Language::Typescript))
        })
        .collect();
    let mut versions = Vec::new();
    for (at, diff) in diffs.iter().enumerate() {
        if let Some(from) = from.filter(|_| diff.status != FileStatus::Added) {
            if let Some(text) = crate::git::file_at(root, from, &diff.old_path) {
                versions.push(serde_json::json!({ "key": format!("{at}:old"), "path": diff.old_path, "text": text }));
            }
        }
        if diff.status != FileStatus::Deleted {
            if let Some(text) = crate::git::file_at(root, to, &diff.path) {
                versions.push(serde_json::json!({ "key": format!("{at}:new"), "path": diff.path, "text": text }));
            }
        }
    }
    if versions.is_empty() {
        return Ok(ImportChanges { judged: true, files: Vec::new() });
    }
    let read: Read = serde_json::from_str(&ask(&versions)?).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for (at, diff) in diffs.iter().enumerate() {
        if let Some(file) = compared(&read, &format!("{at}"), &diff.path) {
            files.push(file);
        }
    }
    Ok(ImportChanges { judged: read.judged, files })
}

/// A file's imports added and removed, from the scanner's answers for its old and new versions (`<key>:old`,
/// `<key>:new`); None when nothing changed.
fn compared(read: &Read, key: &str, path: &str) -> Option<FileImports> {
    let old = read.files.get(&format!("{key}:old")).cloned().unwrap_or_default();
    let new = read.files.get(&format!("{key}:new")).cloned().unwrap_or_default();
    let added: Vec<Import> = new.iter().filter(|edge| !old.iter().any(|known| known.to == edge.to)).cloned().collect();
    let removed: Vec<Import> =
        old.iter().filter(|edge| !new.iter().any(|known| known.to == edge.to)).cloned().collect();
    (!added.is_empty() || !removed.is_empty()).then(|| FileImports { path: path.to_owned(), added, removed })
}

/// A commit of the history to read: where it is in the list, its file versions for the scanner, and each file's key
/// and path.
struct Wanted(usize, Vec<serde_json::Value>, Vec<(String, String)>);

/// The imports and breaks added and removed by each of the last `limit` commits along HEAD's first parents. The
/// commits and their files come from one `git log`, their versions are read in the process, and they go to the scanner
/// a few hundred at a time.
pub fn history(root: &str, limit: u32, ask: &AskImports) -> Result<ArchitectureHistory, String> {
    let found = crate::git::first_parent_changes(root, limit.clamp(1, HISTORY_MOST))?;
    let mut commits: Vec<CommitArchitecture> = found
        .iter()
        .map(|commit| CommitArchitecture {
            sha: commit.info.sha.clone(),
            subject: commit.info.subject.clone(),
            time: commit.info.time,
            added: 0,
            removed: 0,
            added_breaks: 0,
            removed_breaks: 0,
            skipped: false,
        })
        .collect();
    // Each commit's TypeScript-family files, by (commit, file) key; a first commit adds everything it has, and one too
    // big is not read: no use counting either as a change.
    let typescript = |path: &str| language_of(path) == Some(Language::Typescript);
    let mut wanted: Vec<Wanted> = Vec::new();
    for (at, commit) in found.iter().enumerate() {
        let Some(parent) = commit.parent.as_deref().filter(|_| commit.files.len() <= HISTORY_COMMIT_FILES) else {
            commits[at].skipped = true;
            continue;
        };
        let mut versions = Vec::new();
        let mut files = Vec::new();
        for (index, (status, old, new)) in commit.files.iter().enumerate() {
            if !typescript(old) && !typescript(new) {
                continue;
            }
            let key = format!("{at}:{index}");
            if *status != FileStatus::Added {
                if let Some(text) = crate::git::file_at(root, parent, old) {
                    versions.push(serde_json::json!({ "key": format!("{key}:old"), "path": old, "text": text }));
                }
            }
            if *status != FileStatus::Deleted {
                if let Some(text) = crate::git::file_at(root, &commit.info.sha, new) {
                    versions.push(serde_json::json!({ "key": format!("{key}:new"), "path": new, "text": text }));
                }
            }
            files.push((key, new.clone()));
        }
        wanted.push(Wanted(at, versions, files));
    }
    let mut judged = true;
    let mut batch: Vec<&Wanted> = Vec::new();
    let mut flush = |batch: &mut Vec<&Wanted>| {
        let versions: Vec<serde_json::Value> =
            batch.iter().flat_map(|Wanted(_, versions, _)| versions.iter().cloned()).collect();
        let read: Option<Read> = if versions.is_empty() {
            Some(Read { judged: true, files: HashMap::new() })
        } else {
            ask(&versions).ok().and_then(|text| serde_json::from_str(&text).ok())
        };
        for Wanted(at, _, files) in batch.drain(..) {
            let Some(read) = &read else {
                commits[*at].skipped = true;
                continue;
            };
            judged &= read.judged;
            for (key, path) in files {
                if let Some(file) = compared(read, key, path) {
                    let entry = &mut commits[*at];
                    entry.added += file.added.len() as u32;
                    entry.removed += file.removed.len() as u32;
                    entry.added_breaks += file.added.iter().filter(|edge| !edge.breaks.is_empty()).count() as u32;
                    entry.removed_breaks += file.removed.iter().filter(|edge| !edge.breaks.is_empty()).count() as u32;
                }
            }
        }
    };
    let mut size = 0;
    for item in &wanted {
        size += item.1.len();
        batch.push(item);
        if size >= HISTORY_ASK {
            flush(&mut batch);
            size = 0;
        }
    }
    flush(&mut batch);
    Ok(ArchitectureHistory { judged, commits })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Repo;

    /// A stand-in for the scanner: `from './x'` imports `x.ts`; an import of `up.ts` breaks the layers.
    fn ask(files: &[serde_json::Value]) -> Result<String, String> {
        let mut out = serde_json::Map::new();
        for file in files {
            let text = file["text"].as_str().unwrap();
            let edges: Vec<serde_json::Value> = text
                .split("from './")
                .skip(1)
                .map(|rest| {
                    let to = format!("{}.ts", rest.split('\'').next().unwrap());
                    let breaks: Vec<&str> = if to == "up.ts" { vec!["layer"] } else { vec![] };
                    serde_json::json!({ "to": to, "typeOnly": false, "fromPart": "low", "toPart": "x", "breaks": breaks })
                })
                .collect();
            out.insert(file["key"].as_str().unwrap().to_owned(), edges.into());
        }
        Ok(serde_json::json!({ "judged": true, "files": out }).to_string())
    }

    #[test]
    fn finds_the_imports_a_commit_adds_and_removes_and_counts_them_over_time() {
        let r = Repo::new();
        r.write("a.ts", "import { b } from './b'\nexport const a = b\n");
        r.write("b.ts", "export const b = 1\n");
        r.write("up.ts", "export const up = 1\n");
        r.write("notes.md", "notes\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        r.write("a.ts", "import { up } from './up'\nexport const a = up\n");
        r.write("notes.md", "more\n");
        r.sh(&["commit", "-q", "-am", "Reach up"]);
        r.write("a.ts", "export const a = 1\n");
        r.sh(&["commit", "-q", "-am", "Drop it"]);
        let shas: Vec<String> = r.sh(&["log", "--format=%H"]).lines().map(str::to_owned).collect();

        let reach = changes(&r.root, Some(&shas[2]), &shas[1], &ask).unwrap();
        assert_eq!(reach.files.len(), 1, "only the TypeScript file, not notes.md");
        let file = &reach.files[0];
        assert_eq!(file.path, "a.ts");
        assert_eq!(
            file.added.iter().map(|edge| (edge.to.as_str(), edge.breaks.len())).collect::<Vec<_>>(),
            [("up.ts", 1)]
        );
        assert_eq!(file.removed.iter().map(|edge| edge.to.as_str()).collect::<Vec<_>>(), ["b.ts"]);

        let history = history(&r.root, 10, &ask).unwrap();
        let seen: Vec<(&str, u32, u32, u32, u32, bool)> = history
            .commits
            .iter()
            .map(|commit| {
                (
                    commit.subject.as_str(),
                    commit.added,
                    commit.removed,
                    commit.added_breaks,
                    commit.removed_breaks,
                    commit.skipped,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [("Drop it", 0, 1, 0, 1, false), ("Reach up", 1, 1, 1, 0, false), ("First", 0, 0, 0, 0, true)]
        );
    }
}
