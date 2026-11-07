//! Watching a repository with the `notify` crate: its root (the files that change what the project is), the folders
//! its scanned files are in, and its git folder. Changes come out debounced, as what to do about them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use notify::event::{AccessKind, AccessMode, EventKind, ModifyKind};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

// Without the file id cache: building it walks every folder watched recursively before the watch starts (20 seconds on
// a large repository), and it only serves to pair the two halves of a rename, which a rescan does not need.
pub type Watcher = Debouncer<notify::RecommendedWatcher, NoCache>;

/// What a change asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Scan again; the reason names the file.
    Scan(String),
    /// Start a new scanner process: code it has loaded changed (the project's rules in code, the scanner itself).
    Restart(String),
    /// Something in git changed (a commit, a branch switch, staging, a fetch): which parts, each once.
    Git(Vec<GitPart>),
    /// The repository's git config changed: its filters are read again, then as every part of git.
    GitConfig,
    /// A folder was made where each folder is watched on its own (Linux): the folders are watched again, then scanned.
    NewFolder(String),
    /// The root's `.git` came or went (`git init`, a clone moved in): its git folder is found and watched again.
    GitFolder,
}

/// Whether each folder is watched on its own. On Linux a recursive watch is one inotify watch per folder underneath,
/// dependencies and builds included, which can pass the system's limit; there they are set folder by folder, without
/// the folders no scan reads. Windows and macOS watch a whole tree with one handle.
fn per_folder() -> bool {
    cfg!(target_os = "linux")
}

/// Set when the system refused more watches; read (and cleared) by `limit_reached`.
static LIMIT: AtomicBool = AtomicBool::new(false);

/// Whether a watch failed on the system's limit since this was last asked.
pub fn limit_reached() -> bool {
    LIMIT.swap(false, Ordering::Relaxed)
}

fn report(path: &Path, error: notify::Error) {
    if matches!(error.kind, notify::ErrorKind::MaxFilesWatch)
        || matches!(&error.kind, notify::ErrorKind::Io(io) if io.raw_os_error() == Some(28))
    {
        LIMIT.store(true, Ordering::Relaxed);
    }
    eprintln!("[haunt] cannot watch {}: {error}", path.display());
}

/// Every folder under `dir` a scan may read, `dir` first: not ignored, not hidden, links not followed.
fn read_folders(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_path_buf()];
    let mut at = 0;
    while at < out.len() {
        if let Ok(entries) = std::fs::read_dir(&out[at]) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
                if folder && !name.starts_with('.') && !crate::paths::ignored(Path::new(name.as_ref())) {
                    out.push(entry.path());
                }
            }
        }
        at += 1;
    }
    out
}

/// A part of git that changed; each view reads again only for the parts it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPart {
    /// HEAD: a commit, a branch switch, a reset.
    Head,
    /// Branches, remote branches and tags: a commit, a fetch, a new branch.
    Refs,
    /// The index: staging (and a commit).
    Index,
    Stash,
}

impl GitPart {
    pub const ALL: [GitPart; 4] = [GitPart::Head, GitPart::Refs, GitPart::Index, GitPart::Stash];

    pub fn name(self) -> &'static str {
        match self {
            GitPart::Head => "head",
            GitPart::Refs => "refs",
            GitPart::Index => "index",
            GitPart::Stash => "stash",
        }
    }
}

/// Whether an event is something changing, not something being read. On Linux inotify also reports a file or folder
/// being opened, and its access time being set: the scanner reading the files would otherwise start the next scan,
/// over and over, and git reading its folder would announce news every second.
fn changes_something(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        EventKind::Modify(ModifyKind::Metadata(_)) => false,
        _ => true,
    }
}

fn watcher(
    delay: Duration,
    path: &Path,
    mode: RecursiveMode,
    mut classify: impl FnMut(&Path) -> Option<Change> + Send + 'static,
    on: impl Fn(Change) + Send + Sync + 'static,
) -> Option<Watcher> {
    let handle = move |result: DebounceEventResult| {
        let Ok(events) = result else { return };
        let mut wanted: Vec<Change> = Vec::new();
        for event in events.iter().filter(|event| changes_something(&event.kind)) {
            for path in &event.paths {
                if let Some(change) = classify(path) {
                    // Git's parts of one burst gather into one change.
                    if let Change::Git(parts) = &change {
                        if let Some(Change::Git(known)) =
                            wanted.iter_mut().find(|known| matches!(known, Change::Git(_)))
                        {
                            known
                                .extend(parts.iter().filter(|part| !known.contains(part)).copied().collect::<Vec<_>>());
                            continue;
                        }
                    }
                    // A restart covers a scan, so it wins; the rest once each.
                    if matches!(change, Change::Restart(_)) {
                        wanted.retain(|known| !matches!(known, Change::Scan(_)));
                    }
                    if !wanted.iter().any(|known| std::mem::discriminant(known) == std::mem::discriminant(&change)) {
                        wanted.push(change);
                    }
                }
            }
        }
        if wanted.iter().any(|change| matches!(change, Change::Restart(_))) {
            wanted.retain(|change| !matches!(change, Change::Scan(_)));
        }
        for change in wanted {
            on(change);
        }
    };
    let mut debouncer =
        new_debouncer_opt::<_, notify::RecommendedWatcher, _>(delay, None, handle, NoCache, notify::Config::default())
            .map_err(|error| eprintln!("[haunt] cannot watch {}: {error}", path.display()))
            .ok()?;
    debouncer.watch(path, mode).map_err(|error| report(path, error)).ok()?;
    Some(debouncer)
}

fn relative(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

/// The root itself, not its folders: its manifests and config, and source files that sit at the root.
pub fn root(root: &Path, on: impl Fn(Change) + Send + Sync + 'static) -> Option<Watcher> {
    let base = root.to_path_buf();
    watcher(
        Duration::from_millis(250),
        root,
        RecursiveMode::NonRecursive,
        move |path| {
            let name = relative(&base, path)?.to_string_lossy().into_owned();
            if name.contains(['/', '\\']) {
                return None;
            }
            if name == ".git" {
                return Some(Change::GitFolder);
            }
            // A submodule added, removed or moved: the submodules are read with the index.
            if name == ".gitmodules" {
                return Some(Change::Git(vec![GitPart::Index]));
            }
            // haunt.config.json may name another architecture script: start afresh.
            if name == "haunt.config.json" {
                Some(Change::Restart(name))
            } else if crate::paths::root_file(&name) || crate::paths::source(Path::new(&name)) {
                Some(Change::Scan(name))
            } else {
                None
            }
        },
        on,
    )
}

/// The folders the scanned files are in (relative to the root), as the last scan reported them.
pub fn folders(
    root: &Path,
    folders: &[String],
    restart_on: Vec<String>,
    on: impl Fn(Change) + Send + Sync + Clone + 'static,
) -> Vec<Watcher> {
    folders
        .iter()
        .filter(|folder| !folder.is_empty() && folder.as_str() != ".")
        .filter_map(|folder| {
            let dir = root.join(folder);
            if !dir.is_dir() {
                return None;
            }
            let base = root.to_path_buf();
            let rules = restart_on.clone();
            let one_by_one = per_folder();
            let mut watched = watcher(
                Duration::from_millis(250),
                &dir,
                if one_by_one { RecursiveMode::NonRecursive } else { RecursiveMode::Recursive },
                move |path| {
                    let inner = relative(&base, path)?;
                    if crate::paths::ignored(&inner) {
                        return None;
                    }
                    if one_by_one && path.is_dir() {
                        let name = inner.file_name()?.to_string_lossy();
                        return (!name.starts_with('.'))
                            .then(|| Change::NewFolder(inner.to_string_lossy().replace('\\', "/")));
                    }
                    if !crate::paths::source(&inner) {
                        return None;
                    }
                    let changed = inner.to_string_lossy().replace('\\', "/");
                    let rule = rules.iter().any(|dir| changed == *dir || changed.starts_with(&format!("{dir}/")));
                    Some(if rule { Change::Restart(changed) } else { Change::Scan(changed) })
                },
                on.clone(),
            )?;
            if one_by_one {
                for inner in read_folders(&dir).into_iter().skip(1) {
                    if let Err(error) = watched.watch(&inner, RecursiveMode::NonRecursive) {
                        report(&inner, error);
                        break;
                    }
                }
            }
            Some(watched)
        })
        .collect()
}

/// The files git leaves while a merge, a rebase, a cherry-pick, a revert or a bisect is under way; they come and go
/// with the operation, which is read with HEAD.
const UNDER_WAY: [&str; 7] =
    ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply", "BISECT_LOG", "BISECT_START"];

/// Which part of git a changed path in the git folder is (`name` relative to it, with forward slashes).
fn git_change(name: &str) -> Option<Change> {
    let part = match name {
        "config" | "config.worktree" => return Some(Change::GitConfig),
        "HEAD" | "ORIG_HEAD" => GitPart::Head,
        _ if UNDER_WAY.contains(&name) || name.starts_with("BISECT_") => GitPart::Head,
        "index" => GitPart::Index,
        "FETCH_HEAD" | "packed-refs" => GitPart::Refs,
        "refs/stash" => GitPart::Stash,
        _ if name.starts_with("refs/") => GitPart::Refs,
        // A worktree added or removed: the worktrees are read with the refs.
        "worktrees" => GitPart::Refs,
        _ if name.starts_with("worktrees/") && name.matches('/').count() == 1 => GitPart::Refs,
        _ => return None,
    };
    Some(Change::Git(vec![part]))
}

/// What a changed path in the common git folder is, for a linked worktree: only what the worktrees share (the
/// branches, the config, the worktrees); the HEAD and index there are the main worktree's.
fn common_change(name: &str) -> Option<Change> {
    let shared = matches!(name, "config" | "packed-refs" | "worktrees")
        || name.starts_with("refs/")
        || name.starts_with("worktrees/");
    shared.then(|| git_change(name)).flatten()
}

/// A repository's git folder, and its common folder: the same folder, except for a linked worktree, whose branches,
/// tags, config and other worktrees are in the main repository's git folder.
#[derive(Debug, Clone, PartialEq)]
pub struct GitFolders {
    pub git: PathBuf,
    pub common: PathBuf,
}

impl GitFolders {
    pub fn of(git: PathBuf) -> GitFolders {
        let common = std::fs::read_to_string(git.join("commondir"))
            .ok()
            .and_then(|common| std::fs::canonicalize(git.join(common.trim())).ok())
            .map(crate::paths::plain)
            .unwrap_or_else(|| git.clone());
        GitFolders { git, common }
    }

    /// What each part of git looks like on disk, in `GitPart::ALL`'s order and then the config: a part whose stamp
    /// changed, changed. Only the times and sizes of the files git writes for it are read, which is cheap enough for
    /// each time the window gets the focus.
    pub fn stamps(&self, root: &Path) -> [u64; 5] {
        use std::hash::{Hash, Hasher};
        let stamp = |files: &[PathBuf]| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for file in files {
                let meta = std::fs::metadata(file).ok();
                file.hash(&mut hasher);
                meta.as_ref().map(|meta| meta.len()).hash(&mut hasher);
                meta.and_then(|meta| meta.modified().ok()).hash(&mut hasher);
            }
            hasher.finish()
        };
        let (git, common) = (&self.git, &self.common);
        let mut head = vec![git.join("HEAD"), git.join("ORIG_HEAD")];
        head.extend(UNDER_WAY.iter().map(|name| git.join(name)));
        // Every branch and tag file, and the worktrees there are.
        let mut refs = vec![common.join("packed-refs"), git.join("FETCH_HEAD")];
        let worktrees = common.join("worktrees");
        let mut folders = vec![common.join("refs"), worktrees.clone()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else { continue };
            for entry in entries.flatten() {
                let inside = entry.file_type().is_ok_and(|kind| kind.is_dir()) && folder != worktrees;
                if inside {
                    folders.push(entry.path());
                } else {
                    refs.push(entry.path());
                }
            }
        }
        refs.sort();
        [
            stamp(&head),
            stamp(&refs),
            stamp(&[git.join("index"), root.join(".gitmodules")]),
            stamp(&[common.join("refs").join("stash"), common.join("logs").join("refs").join("stash")]),
            stamp(&[common.join("config"), git.join("config.worktree")]),
        ]
    }
}

/// The repository's git folder: HEAD (a branch switch), the index (staging, a commit), its config, refs (a commit, a
/// fetch, a new branch, a stash), its worktrees, and the files of a merge, rebase or bisect under way. For a linked
/// worktree also the common folder, where the branches are. One watcher for all, so a commit is mostly one piece of
/// news (the debouncer reports each path once it has been quiet, so two paths can land a tick apart; each view reads
/// again only for its parts).
pub fn git(folders: &GitFolders, on: impl Fn(Change) + Send + Sync + Clone + 'static) -> Vec<Watcher> {
    let (git, common) = (folders.git.clone(), folders.common.clone());
    let classify = move |path: &Path| {
        let name = |base: &Path| relative(base, path).map(|name| name.to_string_lossy().replace('\\', "/"));
        match name(&git) {
            Some(name) => git_change(&name),
            None => common_change(&name(&common)?),
        }
    };
    let Some(mut watched) =
        watcher(Duration::from_millis(400), &folders.git, RecursiveMode::NonRecursive, classify, on)
    else {
        return Vec::new();
    };
    let mut also = |path: PathBuf, mode: RecursiveMode| {
        if path.is_dir() {
            let _ = watched.watch(&path, mode).map_err(|error| report(&path, error));
        }
    };
    if folders.common != folders.git {
        also(folders.common.clone(), RecursiveMode::NonRecursive);
    }
    also(folders.common.join("refs"), RecursiveMode::Recursive);
    also(folders.common.join("worktrees"), RecursiveMode::NonRecursive);
    vec![watched]
}

/// The folders the repository manager lists repositories from (`folders::looked_in`), each on its own: `on` when a
/// folder came or went in one (a clone, a repository moved or deleted) or a `.git` did (`git init`), not for files.
pub fn repository_folders(dirs: &[PathBuf], on: impl Fn() + Send + Sync + 'static) -> Option<Watcher> {
    let (first, rest) = dirs.split_first()?;
    let classify = |path: &Path| {
        let name = path.file_name()?.to_string_lossy();
        let folder = name == ".git" || path.is_dir() || !path.exists();
        (folder && (name == ".git" || !name.starts_with('.'))).then(|| Change::Scan(name.into_owned()))
    };
    let mut watched = watcher(Duration::from_millis(800), first, RecursiveMode::NonRecursive, classify, move |_| on())?;
    for dir in rest {
        let _ = watched.watch(dir, RecursiveMode::NonRecursive).map_err(|error| report(dir, error));
    }
    Some(watched)
}

/// One file, by watching its folder alone (an editor or a tool may write a new file in its place): `on` after each
/// change to it.
pub fn file(file: &Path, on: impl Fn() + Send + Sync + 'static) -> Option<Watcher> {
    let folder = file.parent()?;
    let name = file.file_name()?.to_owned();
    watcher(
        Duration::from_millis(200),
        folder,
        RecursiveMode::NonRecursive,
        move |path| (path.file_name() == Some(name.as_os_str())).then(|| Change::Scan(String::new())),
        move |_| on(),
    )
}

/// The scanner's own code, while Haunt itself is worked on: a change needs a new scanner process.
#[cfg(debug_assertions)]
pub fn sidecar(dir: &Path, on: impl Fn(Change) + Send + Sync + 'static) -> Option<Watcher> {
    watcher(
        Duration::from_millis(250),
        dir,
        RecursiveMode::Recursive,
        |path| {
            let name = path.to_string_lossy();
            (name.ends_with(".ts") && !name.contains("fixtures"))
                .then(|| Change::Restart("a change to the scanner".into()))
        },
        on,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// On Linux each folder is watched on its own: every folder a scan may read, none of the ignored or hidden ones.
    #[test]
    fn lists_the_folders_to_watch_one_by_one() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-folders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for folder in ["src/a/b", "src/node_modules/x", "src/.cache", "src/target", "lib"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
        }
        let mut found: Vec<String> = read_folders(&dir)
            .iter()
            .map(|path| path.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        found.sort();
        assert_eq!(found, ["", "lib", "src", "src/a", "src/a/b"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_a_saved_file_once_as_a_scan() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-{}", std::process::id()));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let root = crate::paths::plain(std::fs::canonicalize(&dir).unwrap());
        let (sender, receiver) = channel();
        let sender = parking_lot::Mutex::new(sender);
        let on = move |change: Change| {
            let _ = sender.lock().send(change);
        };
        let shared = std::sync::Arc::new(on);
        let handler = {
            let shared = shared.clone();
            move |change: Change| shared(change)
        };
        let _watchers = folders(&root, &["src".into()], vec![], handler);
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(src.join("a.ts"), "export const a = 1\n").unwrap();
        std::fs::write(src.join("a.ts"), "export const a = 2\n").unwrap();
        std::fs::write(src.join("notes.md"), "not code\n").unwrap();
        let first = receiver.recv_timeout(Duration::from_secs(5)).expect("a change");
        assert_eq!(first, Change::Scan("src/a.ts".into()));
        assert!(
            receiver.recv_timeout(Duration::from_millis(800)).is_err(),
            "one scan for both writes, none for the notes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_files_and_folders_is_not_a_change() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-read-{}", std::process::id()));
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("a.ts"), "export const a = 1\n").unwrap();
        let root = crate::paths::plain(std::fs::canonicalize(&dir).unwrap());
        let (sender, receiver) = channel();
        let sender = std::sync::Arc::new(parking_lot::Mutex::new(sender));
        let _watchers = folders(&root, &["src".into()], vec![], move |change: Change| {
            let _ = sender.lock().send(change);
        });
        std::thread::sleep(Duration::from_millis(200));
        // macOS's FSEvents may still deliver the writes made above, after the watch started: wait until it has been
        // quiet for a second, so only what the reads below cause is checked.
        let quiet = std::time::Instant::now();
        while receiver.recv_timeout(Duration::from_millis(1000)).is_ok() {
            assert!(quiet.elapsed() < Duration::from_secs(15), "the setup's writes kept coming");
        }
        // What a scan does: list the folders and read the files.
        for _ in 0..3 {
            let _ = std::fs::read_dir(&src).unwrap().count();
            let _ = std::fs::read_dir(src.join("inner")).unwrap().count();
            let _ = std::fs::read_to_string(src.join("a.ts")).unwrap();
        }
        assert!(receiver.recv_timeout(Duration::from_millis(1000)).is_err(), "reading started a scan");
        // A write still does.
        std::fs::write(src.join("a.ts"), "export const a = 2\n").unwrap();
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).expect("a change"), Change::Scan("src/a.ts".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tells_a_change_from_a_read() {
        use notify::event::{AccessKind, CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind};
        assert!(!changes_something(&EventKind::Access(AccessKind::Open(AccessMode::Any))));
        assert!(!changes_something(&EventKind::Access(AccessKind::Close(AccessMode::Read))));
        assert!(!changes_something(&EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))));
        assert!(changes_something(&EventKind::Access(AccessKind::Close(AccessMode::Write))));
        assert!(changes_something(&EventKind::Modify(ModifyKind::Data(DataChange::Any))));
        assert!(changes_something(&EventKind::Create(CreateKind::Folder)));
        assert!(changes_something(&EventKind::Remove(RemoveKind::File)));
        assert!(changes_something(&EventKind::Any));
    }

    #[test]
    fn names_the_part_of_git_a_path_is() {
        let part = |name: &str| git_change(name);
        assert_eq!(part("HEAD"), Some(Change::Git(vec![GitPart::Head])));
        assert_eq!(part("index"), Some(Change::Git(vec![GitPart::Index])));
        assert_eq!(part("refs/heads/feature/x"), Some(Change::Git(vec![GitPart::Refs])));
        assert_eq!(part("FETCH_HEAD"), Some(Change::Git(vec![GitPart::Refs])));
        assert_eq!(part("refs/stash"), Some(Change::Git(vec![GitPart::Stash])));
        assert_eq!(part("config"), Some(Change::GitConfig));
        assert_eq!(part("objects/ab/cdef"), None);
        assert_eq!(part("index.lock"), None);
        // A merge, rebase or bisect starting or ending is read with HEAD.
        assert_eq!(part("MERGE_HEAD"), Some(Change::Git(vec![GitPart::Head])));
        assert_eq!(part("rebase-merge"), Some(Change::Git(vec![GitPart::Head])));
        assert_eq!(part("BISECT_EXPECTED_REV"), Some(Change::Git(vec![GitPart::Head])));
        // A worktree added or removed, not what changes inside one.
        assert_eq!(part("worktrees/feature"), Some(Change::Git(vec![GitPart::Refs])));
        assert_eq!(part("worktrees/feature/HEAD"), None);
        assert_eq!(part("config.worktree"), Some(Change::GitConfig));
    }

    #[test]
    fn a_linked_worktree_takes_only_what_is_shared_from_the_common_folder() {
        assert_eq!(common_change("refs/heads/main"), Some(Change::Git(vec![GitPart::Refs])));
        assert_eq!(common_change("packed-refs"), Some(Change::Git(vec![GitPart::Refs])));
        assert_eq!(common_change("config"), Some(Change::GitConfig));
        assert_eq!(common_change("worktrees/other"), Some(Change::Git(vec![GitPart::Refs])));
        // The main worktree's own.
        assert_eq!(common_change("HEAD"), None);
        assert_eq!(common_change("index"), None);
        assert_eq!(common_change("MERGE_HEAD"), None);
    }

    #[test]
    fn stamps_change_with_the_part_that_changed() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-stamps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let git = dir.join(".git");
        let heads = git.join("refs").join("heads");
        std::fs::create_dir_all(&heads).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let folders = GitFolders::of(git.clone());
        assert_eq!(folders.common, git, "not a linked worktree");
        let changed = |before: [u64; 5]| {
            let now = folders.stamps(&dir);
            (0..5).filter(|at| before[*at] != now[*at]).collect::<Vec<_>>()
        };
        let before = folders.stamps(&dir);
        assert!(changed(before).is_empty(), "nothing changed, the same stamps");
        std::fs::write(heads.join("feature"), "0000000000000000000000000000000000000000\n").unwrap();
        assert_eq!(changed(before), [1], "a new branch: the refs");
        let before = folders.stamps(&dir);
        std::fs::create_dir(git.join("rebase-merge")).unwrap();
        assert_eq!(changed(before), [0], "a rebase under way: HEAD's part");
        let before = folders.stamps(&dir);
        std::fs::write(dir.join(".gitmodules"), "[submodule]").unwrap();
        assert_eq!(changed(before), [2], "a submodule: the index's part");
        let before = folders.stamps(&dir);
        std::fs::create_dir_all(git.join("worktrees").join("feature")).unwrap();
        assert_eq!(changed(before), [1], "a worktree: the refs");
        let before = folders.stamps(&dir);
        std::fs::write(git.join("config"), "[core]").unwrap();
        assert_eq!(changed(before), [4], "the config");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_linked_worktree_finds_its_common_folder() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-common-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let own = dir.join(".git").join("worktrees").join("feature");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("commondir"), "../..\n").unwrap();
        let folders = GitFolders::of(crate::paths::plain(std::fs::canonicalize(&own).unwrap()));
        let common = crate::paths::plain(std::fs::canonicalize(dir.join(".git")).unwrap());
        assert_eq!(folders.common, common);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_a_commit_with_its_parts() {
        let dir = std::env::temp_dir().join(format!("haunt-watch-git-{}", std::process::id()));
        let heads = dir.join("refs").join("heads");
        std::fs::create_dir_all(&heads).unwrap();
        let git_dir = crate::paths::plain(std::fs::canonicalize(&dir).unwrap());
        let (sender, receiver) = channel();
        let sender = std::sync::Arc::new(parking_lot::Mutex::new(sender));
        let _watchers = git(&GitFolders::of(git_dir), move |change: Change| {
            let _ = sender.lock().send(change);
        });
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.join("index"), "index").unwrap();
        std::fs::write(heads.join("main"), "0000000000000000000000000000000000000000\n").unwrap();
        // Mostly one change with both parts; two, a tick apart, when the writes fall either side of one.
        let mut parts = Vec::new();
        let mut changes = 0;
        let mut wait = Duration::from_secs(5);
        while let Ok(change) = receiver.recv_timeout(wait) {
            let Change::Git(mut found) = change else { panic!("not a git change") };
            let named = found.len();
            found.sort_by_key(|part| part.name());
            found.dedup();
            assert_eq!(found.len(), named, "a part named twice in one change");
            parts.extend(found);
            changes += 1;
            wait = Duration::from_millis(800);
        }
        // Across two changes a part may come twice: macOS's FSEvents reports the branch's file in both when it splits
        // the burst.
        parts.sort_by_key(|part| part.name());
        parts.dedup();
        assert_eq!(parts, [GitPart::Index, GitPart::Refs], "each part once");
        assert!(changes <= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
