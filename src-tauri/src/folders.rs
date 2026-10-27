//! The folders you keep repositories in (Settings keeps them, for every profile), and the repositories found in each,
//! for the repository manager. A folder is looked through a few levels deep; a repository's own folders are not
//! (a repository inside another is a submodule, shown in its own repository), nor are hidden folders, links, or the
//! dependency and build folders no scan reads.

use std::path::Path;

use serde::Serialize;
use specta::Type;

/// How deep under a folder a repository is looked for: `Projects/client/app` is two levels.
const DEPTH: usize = 4;
/// At most this many repositories per folder: a folder that holds more is cut.
const MOST: usize = 500;

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoFolder {
    pub path: String,
    /// The folder is not there now (an unplugged drive).
    pub missing: bool,
    /// The repositories in it, by their path; sorted.
    pub repos: Vec<String>,
    /// It holds more than Haunt lists.
    pub cut: bool,
}

pub fn read(path: &str) -> RepoFolder {
    if !Path::new(path).is_dir() {
        return RepoFolder { path: path.to_owned(), missing: true, repos: Vec::new(), cut: false };
    }
    let mut repos = Vec::new();
    let cut = look(Path::new(path), 0, &mut repos);
    repos.sort_by_key(|repo: &String| repo.to_lowercase());
    RepoFolder { path: path.to_owned(), missing: false, repos, cut }
}

/// Adds the repositories in `dir` to `found`; true when it stopped at `MOST`.
fn look(dir: &Path, depth: usize, found: &mut Vec<String>) -> bool {
    if dir.join(".git").exists() {
        found.push(dir.to_string_lossy().into_owned());
        return found.len() >= MOST;
    }
    if depth >= DEPTH {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    let mut children: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        // A link is not followed: it could lead back up, or to the same repository twice.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && !crate::paths::ignored(Path::new(name.as_ref()))
        })
        .map(|entry| entry.path())
        .collect();
    children.sort();
    for child in children {
        if look(&child, depth + 1, found) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repositories_without_going_into_them() {
        let base = std::env::temp_dir().join(format!("haunt-folders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for dir in [
            "app/.git",
            "app/packages/inner/.git",
            "client/site/.git",
            "node_modules/lib/.git",
            ".hidden/x/.git",
            "empty",
        ] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        let folder = read(&base.to_string_lossy());
        let names: Vec<String> = folder
            .repos
            .iter()
            .map(|repo| Path::new(repo).strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(names, ["app", "client/site"]);
        assert!(!folder.missing && !folder.cut);
        assert!(read(&base.join("gone").to_string_lossy()).missing);
        let _ = std::fs::remove_dir_all(&base);
    }
}
