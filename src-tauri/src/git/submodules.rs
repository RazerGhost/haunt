//! A repository's submodules: where each is, where it comes from, the commit the repository records for it and the one
//! checked out. Read without `git submodule`: the index's gitlinks (`ls-files -s`), `.gitmodules`, and each checked-out
//! submodule's HEAD. Updating them is a write (`write/network.rs`, `update_submodules`).

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use super::cli::git;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SubmoduleState {
    /// Checked out at the commit the repository records.
    Current,
    /// Checked out at another commit: updated since, or not yet updated after a pull.
    Moved,
    /// Not checked out yet (never initialised, or its folder is empty).
    Missing,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Submodule {
    /// Its folder, relative to the repository.
    pub path: String,
    /// Its name and where it is cloned from, in `.gitmodules`; empty when not listed there.
    pub name: String,
    pub url: String,
    /// The commit the repository records for it.
    pub recorded: String,
    /// The commit checked out in its folder, when it is.
    pub checked_out: Option<String>,
    pub state: SubmoduleState,
}

/// The submodules of `root`, by path.
pub fn submodules(root: &str) -> Result<Vec<Submodule>, String> {
    let listing = git(root, &["ls-files", "-s", "-z"])?;
    // `.gitmodules`: submodule.<name>.path and .url.
    let mut by_name: HashMap<String, (String, String)> = HashMap::new();
    if Path::new(root).join(".gitmodules").is_file() {
        let config = git(root, &["config", "-f", ".gitmodules", "-z", "--get-regexp", r"^submodule\..*\.(path|url)$"])
            .unwrap_or_default();
        for entry in config.split('\0').filter(|entry| !entry.is_empty()) {
            let Some((key, value)) = entry.split_once('\n') else { continue };
            let Some(rest) = key.strip_prefix("submodule.") else { continue };
            let Some((name, field)) = rest.rsplit_once('.') else { continue };
            let slot = by_name.entry(name.to_owned()).or_default();
            if field == "path" {
                slot.0 = value.to_owned();
            } else {
                slot.1 = value.to_owned();
            }
        }
    }
    let mut out = Vec::new();
    for entry in listing.split('\0') {
        // "<mode> <sha> <stage>\t<path>"
        let Some((meta, path)) = entry.split_once('\t') else { continue };
        let mut meta = meta.split(' ');
        if meta.next() != Some("160000") {
            continue;
        }
        let recorded = meta.next().unwrap_or_default().to_owned();
        let (name, url) = by_name
            .iter()
            .find(|(_, (at, _))| at == path)
            .map(|(name, (_, url))| (name.clone(), url.clone()))
            .unwrap_or_default();
        let folder = Path::new(root).join(path);
        let checked_out = folder
            .join(".git")
            .exists()
            .then(|| git(&folder.to_string_lossy(), &["rev-parse", "--verify", "--quiet", "HEAD"]).ok())
            .flatten()
            .map(|sha| sha.trim().to_owned())
            .filter(|sha| !sha.is_empty());
        let state = match &checked_out {
            None => SubmoduleState::Missing,
            Some(sha) if *sha == recorded => SubmoduleState::Current,
            Some(_) => SubmoduleState::Moved,
        };
        out.push(Submodule { path: path.to_owned(), name, url, recorded, checked_out, state });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
