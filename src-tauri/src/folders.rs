//! The folders you keep repositories in (Settings keeps them, for every profile), and the repositories found in each,
//! for the repository manager. A folder is looked through a few levels deep; a repository's own folders are not
//! (a repository inside another is a submodule, shown in its own repository), nor are hidden folders, links, or the
//! dependency and build folders no scan reads. A search walks a lot of folders, so its result is kept and served at
//! once (`Searched`), and searched again in the background now and then.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use specta::Type;

/// How deep under a folder a repository is looked for: `Projects/client/app` is two levels.
const DEPTH: usize = 4;
/// At most this many repositories per folder: a folder that holds more is cut.
const MOST: usize = 500;
/// How long a folder's last search is served as it is; after that it is served once more while searched again.
const FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
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

/// A folder's last search: when it started, what it found, and whether it is being searched again now.
struct Kept {
    at: Instant,
    folder: RepoFolder,
    searching: bool,
}

/// The folders as last searched, by path, shared with the background searches.
#[derive(Clone)]
pub struct Searched {
    kept: Arc<Mutex<HashMap<String, Kept>>>,
    search: Arc<dyn Fn(&str) -> RepoFolder + Send + Sync>,
}

impl Default for Searched {
    fn default() -> Searched {
        Searched::with(read)
    }
}

impl Searched {
    fn with(search: impl Fn(&str) -> RepoFolder + Send + Sync + 'static) -> Searched {
        Searched { kept: Arc::default(), search: Arc::new(search) }
    }

    /// The folders `paths`, in their order, each as last searched; one never searched is searched now (side by side).
    /// One searched more than `FRESH_FOR` ago, or every one when `again`, is searched again in the background, and
    /// `changed` is called when that found a different list.
    pub fn list(&self, paths: &[String], again: bool, changed: impl Fn() + Send + 'static) -> Vec<RepoFolder> {
        let mut out: Vec<Option<RepoFolder>> = Vec::new();
        let mut stale: Vec<String> = Vec::new();
        {
            let mut kept = self.kept.lock();
            kept.retain(|path, _| paths.contains(path));
            for path in paths {
                out.push(kept.get_mut(path).map(|known| {
                    if (again || known.at.elapsed() >= FRESH_FOR) && !known.searching {
                        known.searching = true;
                        stale.push(path.clone());
                    }
                    known.folder.clone()
                }));
            }
        }
        let started = Instant::now();
        let new: Vec<usize> = (0..paths.len()).filter(|&index| out[index].is_none()).collect();
        let found = self.search_all(new.iter().map(|&index| paths[index].as_str()).collect());
        {
            let mut kept = self.kept.lock();
            for (&index, folder) in new.iter().zip(found) {
                let folder = folder.unwrap_or_else(|| (self.search)(&paths[index]));
                kept.insert(paths[index].clone(), Kept { at: started, folder: folder.clone(), searching: false });
                out[index] = Some(folder);
            }
        }
        if !stale.is_empty() {
            let me = self.clone();
            std::thread::spawn(move || me.search_again(stale, changed));
        }
        out.into_iter().flatten().collect()
    }

    /// Searches `paths` side by side; None for a search that panicked.
    fn search_all(&self, paths: Vec<&str>) -> Vec<Option<RepoFolder>> {
        std::thread::scope(|scope| {
            let searching: Vec<_> = paths.iter().map(|path| scope.spawn(|| (self.search)(path))).collect();
            searching.into_iter().map(|search| search.join().ok()).collect()
        })
    }

    /// Searches `paths` again and keeps what was found; `changed` when a list is not what was kept.
    fn search_again(&self, paths: Vec<String>, changed: impl Fn()) {
        let started = Instant::now();
        let found = self.search_all(paths.iter().map(String::as_str).collect());
        let mut different = false;
        {
            let mut kept = self.kept.lock();
            for (path, folder) in paths.iter().zip(found) {
                // A folder removed meanwhile stays removed.
                let Some(known) = kept.get_mut(path) else { continue };
                known.searching = false;
                if let Some(folder) = folder {
                    different |= known.folder != folder;
                    known.folder = folder;
                    known.at = started;
                }
            }
        }
        if different {
            changed();
        }
    }
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

    #[test]
    fn serves_the_last_search_and_searches_again_in_the_background() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let base = std::env::temp_dir().join(format!("haunt-searched-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a/.git")).unwrap();
        let walks = Arc::new(AtomicUsize::new(0));
        let searched = Searched::with({
            let walks = walks.clone();
            move |path| {
                walks.fetch_add(1, Ordering::SeqCst);
                read(path)
            }
        });
        let paths = vec![base.to_string_lossy().into_owned()];
        let names = |folders: Vec<RepoFolder>| {
            folders[0]
                .repos
                .iter()
                .map(|repo| Path::new(repo).file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let (tell, told) = std::sync::mpsc::channel();
        let changed = move || {
            let tell = tell.clone();
            move || tell.send(()).unwrap()
        };

        assert_eq!(names(searched.list(&paths, false, changed())), ["a"]);
        assert_eq!(names(searched.list(&paths, false, changed())), ["a"]);
        assert_eq!(walks.load(Ordering::SeqCst), 1, "the second call is served from the last search");

        // Asked to search again: the last search at once, the new repository once the background search found it.
        std::fs::create_dir_all(base.join("b/.git")).unwrap();
        assert_eq!(names(searched.list(&paths, true, changed())), ["a"]);
        told.recv_timeout(Duration::from_secs(10)).expect("the window hears of the new list");
        assert_eq!(walks.load(Ordering::SeqCst), 2);
        assert_eq!(names(searched.list(&paths, false, changed())), ["a", "b"]);
        assert_eq!(walks.load(Ordering::SeqCst), 2);

        // An old search is searched again too; the same list says nothing.
        if let Some(old) = Instant::now().checked_sub(FRESH_FOR) {
            searched.kept.lock().get_mut(&paths[0]).unwrap().at = old;
            assert_eq!(names(searched.list(&paths, false, changed())), ["a", "b"]);
            for _ in 0..100 {
                if !searched.kept.lock()[&paths[0]].searching {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(walks.load(Ordering::SeqCst), 3);
            assert!(told.try_recv().is_err());
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
