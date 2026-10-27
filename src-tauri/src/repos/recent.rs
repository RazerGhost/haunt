//! The recent repositories and the repository folders, as the repository manager shows them, and a typed path's check.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
use specta::Type;

use super::{Repos, RootInfo};
use crate::paths;

/// A recent repository, as the open screen shows it: its folder's name, and, when it is a git repository, its branch,
/// its uncommitted files and its last commit.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepo {
    pub path: String,
    pub name: String,
    /// The folder is still there, and a project or a git repository.
    pub openable: bool,
    pub branch: Option<String>,
    pub changes: Option<u32>,
    pub last_commit: Option<crate::git::types::CommitInfo>,
}

impl RecentRepo {
    fn read(path: &str) -> RecentRepo {
        let openable = paths::openable(Path::new(path));
        let git = openable && crate::git::diff::is_repository(path);
        RecentRepo {
            path: path.to_owned(),
            name: Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned()),
            openable,
            branch: git.then(|| crate::git::diff::head(path).branch).flatten(),
            changes: git.then(|| crate::git::local_changes(path).ok().flatten()).flatten(),
            last_commit: git
                .then(|| crate::git::first_parents(path, 1).ok().and_then(|commits| commits.into_iter().next()))
                .flatten(),
        }
    }
}

/// What a typed path is, before it is opened: whether it is there, and whether Haunt can open it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderCheck {
    pub exists: bool,
    pub openable: bool,
    pub repository: bool,
}

pub fn check_folder(path: &str) -> FolderCheck {
    match paths::canonical(path) {
        None => FolderCheck { exists: false, openable: false, repository: false },
        Some(root) => FolderCheck {
            exists: true,
            openable: paths::openable(Path::new(&root)),
            repository: Path::new(&root).ancestors().any(|dir| dir.join(".git").exists()),
        },
    }
}

/// How many recent repositories are read at once: each runs `git status`, which takes a while in a big repository.
const AT_ONCE: usize = 4;
/// How long a reading is used again while git's files say nothing changed (a file edited in the working tree changes
/// none of them).
const FRESH_FOR: Duration = Duration::from_secs(60);

/// What a reading of a recent repository depends on: whether its folder is there, and when git last wrote its index,
/// HEAD, packed refs and current branch. Found by reading files only, so nothing of the repository runs.
#[derive(Debug, Clone, PartialEq)]
struct Stamp {
    folder: bool,
    times: Vec<Option<SystemTime>>,
}

impl Stamp {
    fn of(path: &str) -> Stamp {
        let modified = |file: &Path| std::fs::metadata(file).and_then(|meta| meta.modified()).ok();
        let mut times = Vec::new();
        if let Some((git, common)) = git_folders(path) {
            let head = git.join("HEAD");
            times.extend([modified(&git.join("index")), modified(&head), modified(&common.join("packed-refs"))]);
            let branch = std::fs::read_to_string(&head)
                .ok()
                .and_then(|text| text.strip_prefix("ref:").map(|name| name.trim().to_owned()))
                .filter(|name| name.starts_with("refs/") && !name.split('/').any(|part| part == ".."));
            times.push(branch.and_then(|name| modified(&common.join(name))));
        }
        Stamp { folder: Path::new(path).is_dir(), times }
    }
}

/// The git folder of the repository `path` is in, and its common folder (where a linked worktree's refs are).
fn git_folders(path: &str) -> Option<(PathBuf, PathBuf)> {
    for dir in Path::new(path).ancestors() {
        let dot = dir.join(".git");
        if dot.is_dir() {
            return Some((dot.clone(), dot));
        }
        if dot.is_file() {
            // A worktree or a submodule: `.git` is a file, "gitdir: <its git folder>".
            let text = std::fs::read_to_string(&dot).ok()?;
            let git = dir.join(text.strip_prefix("gitdir:")?.trim());
            let common = std::fs::read_to_string(git.join("commondir"))
                .map(|common| git.join(common.trim()))
                .unwrap_or_else(|_| git.clone());
            return Some((git, common));
        }
    }
    None
}

/// The recent repositories as last read, by path: what each reading depended on, when it was made, and what it read.
#[derive(Default)]
pub(super) struct Remembered(Mutex<HashMap<String, (Stamp, Instant, RecentRepo)>>);

impl Remembered {
    /// `paths` read with `read`, in their order: a remembered reading while its stamp holds and it is younger than
    /// `FRESH_FOR`, otherwise read again, at most `AT_ONCE` at a time.
    fn details(&self, paths: &[String], read: impl Fn(&str) -> RecentRepo + Sync) -> Vec<RecentRepo> {
        let stamps: Vec<Stamp> = paths.iter().map(|path| Stamp::of(path)).collect();
        let mut out: Vec<Option<RecentRepo>> = {
            let kept = self.0.lock();
            paths
                .iter()
                .zip(&stamps)
                .map(|(path, stamp)| {
                    kept.get(path)
                        .filter(|(known, at, _)| known == stamp && at.elapsed() < FRESH_FOR)
                        .map(|(_, _, repo)| repo.clone())
                })
                .collect()
        };
        let wanted: Vec<usize> = (0..paths.len()).filter(|&index| out[index].is_none()).collect();
        let started = Instant::now();
        let next = AtomicUsize::new(0);
        let done: Mutex<Vec<(usize, RecentRepo)>> = Mutex::default();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..AT_ONCE.min(wanted.len()))
                .map(|_| {
                    scope.spawn(|| {
                        while let Some(&index) = wanted.get(next.fetch_add(1, Ordering::Relaxed)) {
                            let repo = read(&paths[index]);
                            done.lock().push((index, repo));
                        }
                    })
                })
                .collect();
            // Joined here, so a reading that panicked is read again below rather than taking the others down.
            for worker in workers {
                let _ = worker.join();
            }
        });
        let mut kept = self.0.lock();
        for (index, repo) in done.into_inner() {
            kept.insert(paths[index].clone(), (stamps[index].clone(), started, repo.clone()));
            out[index] = Some(repo);
        }
        kept.retain(|path, _| paths.contains(path));
        drop(kept);
        out.into_iter().zip(paths).map(|(repo, path)| repo.unwrap_or_else(|| read(path))).collect()
    }
}

impl Repos {
    /// The recent repositories of the profile in use, with what the open screen shows of each (only these folders are
    /// read: what the settings already name).
    pub fn recent_details(&self) -> Vec<RecentRepo> {
        let recent = self.inner.lock().settings.recent.clone();
        // Each costs a few git calls (its branch, status and last commit): read a few side by side, in the list's
        // order, and only those git changed since (or a while ago).
        self.recent.details(&recent, RecentRepo::read)
    }

    /// The repository folders, each with the repositories found in it, as last searched (folders.rs).
    pub fn repository_folders(&self) -> Vec<crate::folders::RepoFolder> {
        self.searched_folders(false)
    }

    /// The repository folders as last searched, a new one searched now; an old search, or every one when `again`, is
    /// searched again in the background, and the window hears ("folders") when that found a different list.
    fn searched_folders(&self, again: bool) -> Vec<crate::folders::RepoFolder> {
        let folders = self.inner.lock().settings.repository_folders.clone();
        let me = self.me.clone();
        self.folders.list(&folders, again, move || {
            if let Some(repos) = me.upgrade() {
                repos.news_for_all(json!({ "type": "folders" }));
            }
        })
    }

    pub fn add_repository_folder(&self, path: &str) -> Result<Vec<crate::folders::RepoFolder>, String> {
        {
            let mut inner = self.inner.lock();
            inner.settings.add_repository_folder(path)?;
            inner.settings.save(&self.context.settings);
        }
        Ok(self.searched_folders(true))
    }

    pub fn remove_repository_folder(&self, path: &str) -> Vec<crate::folders::RepoFolder> {
        {
            let mut inner = self.inner.lock();
            inner.settings.remove_repository_folder(path);
            inner.settings.save(&self.context.settings);
        }
        self.searched_folders(true)
    }

    /// Takes a folder off the recent list.
    pub fn forget_recent(&self, path: &str) -> RootInfo {
        {
            let mut inner = self.inner.lock();
            inner.settings.forget_recent(path);
            inner.settings.save(&self.context.settings);
        }
        self.info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_typed_folder_and_reads_a_recent_repository() {
        let r = crate::git::tests::Repo::new();
        r.write("a.ts", "export const a = 1\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        r.write("b.ts", "export const b = 1\n");
        let found = check_folder(&r.root);
        assert!(found.exists && found.openable && found.repository);
        let missing = check_folder(&format!("{}/nowhere", r.root));
        assert!(!missing.exists && !missing.openable);
        let recent = RecentRepo::read(&r.root);
        assert_eq!((recent.branch.as_deref(), recent.changes), (Some("main"), Some(1)));
        assert_eq!(recent.last_commit.unwrap().subject, "First");
    }

    #[test]
    fn reads_a_recent_repository_again_only_when_git_changed_it() {
        let r = crate::git::tests::Repo::new();
        r.write("a.ts", "export const a = 1\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        let reads = AtomicUsize::new(0);
        let read = |path: &str| {
            reads.fetch_add(1, Ordering::SeqCst);
            RecentRepo::read(path)
        };
        let remembered = Remembered::default();
        let paths = vec![r.root.clone()];
        let subject = |details: &[RecentRepo]| details[0].last_commit.as_ref().unwrap().subject.clone();

        assert_eq!(subject(&remembered.details(&paths, read)), "First");
        assert_eq!(subject(&remembered.details(&paths, read)), "First");
        assert_eq!(reads.load(Ordering::SeqCst), 1, "nothing changed: git is not run again");

        r.write("b.ts", "export const b = 1\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Second"]);
        assert_eq!(subject(&remembered.details(&paths, read)), "Second");
        assert_eq!(reads.load(Ordering::SeqCst), 2, "a commit: read again");

        // An old reading is read again too.
        if let Some(old) = Instant::now().checked_sub(FRESH_FOR + Duration::from_secs(1)) {
            remembered.0.lock().get_mut(&r.root).unwrap().1 = old;
            remembered.details(&paths, read);
            assert_eq!(reads.load(Ordering::SeqCst), 3);
        }
    }

    #[test]
    fn reads_a_few_at_a_time_in_the_lists_order() {
        let base = std::env::temp_dir().join(format!("haunt-recent-{}", std::process::id()));
        let paths: Vec<String> =
            (0..10).map(|n| base.join(format!("gone-{n}")).to_string_lossy().into_owned()).collect();
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let read = |path: &str| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            running.fetch_sub(1, Ordering::SeqCst);
            RecentRepo::read(path)
        };
        let details = Remembered::default().details(&paths, read);
        assert_eq!(details.iter().map(|repo| repo.path.clone()).collect::<Vec<_>>(), paths);
        assert!(details.iter().all(|repo| !repo.openable));
        let most = most.load(Ordering::SeqCst);
        assert!((2..=AT_ONCE).contains(&most), "{most} read at once");
    }
}
