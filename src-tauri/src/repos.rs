//! The repositories: the open tabs, the one shown with its scanner process and watchers, trust, and the last scan of a
//! few other tabs, kept in memory so going back to one shows it at once while it is scanned again. The last few of those
//! keep their scanner running too: going back asks it to scan again, which reads only what changed, instead of starting
//! a new one that reads everything. Every call names its repository; a call for one that is not shown is refused, so an
//! answer can never land in the wrong tab.

mod missing;
mod news;
mod recent;
mod scans;
mod settings;
mod tabs;
mod trust;

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Instant;

use serde::Serialize;
use specta::Type;

use crate::events::Events;
use crate::paths;
use crate::settings::Settings;
use crate::sidecar::{Shape, Sidecar};
use crate::startup::Runtime;
use crate::watch::{GitFolders, Watcher};

pub use recent::{check_folder, FolderCheck, RecentRepo};
pub use settings::{AppFolder, AppSettings};
use trust::wants;
pub use trust::{wanted_texts, Wanted, WantedText};

/// How many other tabs keep their last scan in memory.
const KEPT: usize = 4;
/// How many of those keep their scanner running (each is a Node, Bun or Deno process holding a TypeScript program).
const RUNNING: usize = 2;

pub type Reply = Result<String, String>;

/// The repository shown, the open tabs, the recent ones, whether its own code may run and what it would run.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RootInfo {
    pub root: Option<String>,
    /// The tabs and recent repositories whose folders are there (what can be shown).
    pub tabs: Vec<String>,
    pub recent: Vec<String>,
    /// Every tab and recent repository, in order, those whose folder is missing (`missing`) too.
    pub all_tabs: Vec<String>,
    pub all_recent: Vec<String>,
    /// The tabs and recent repositories whose folder is missing (moved, deleted, a drive not connected).
    pub missing: Vec<String>,
    pub trusted: bool,
    /// What the folder would run if trusted; empty when it has nothing to run.
    pub wants: Vec<Wanted>,
    /// The tabs that are linked worktrees of a repository (their `.git` is a file), to mark them.
    pub worktrees: Vec<String>,
    /// The profile in use, and every profile's name.
    pub profile: String,
    pub profiles: Vec<String>,
    /// How the profile in use looks.
    pub appearance: crate::settings::Appearance,
    /// The open tabs that are pinned or coloured, by folder.
    pub tab_looks: HashMap<String, crate::settings::TabLook>,
}

/// What Haunt needs to run a scanner.
pub struct Context {
    /// What runs the scanner (Node, Bun or Deno), or why none can (then nothing is scanned, and every call says why).
    pub runtime: Result<Runtime, String>,
    /// `sidecar/main.ts`.
    pub script: PathBuf,
    pub settings: PathBuf,
    /// Where the scanners' messages go in a release build (the console while Haunt is worked on).
    pub log: Option<PathBuf>,
    /// Where the scanners keep their call graphs between launches (the app's cache folder); none in the tests.
    pub cache: Option<PathBuf>,
    /// The app's log folder, for the Settings window; none while Haunt is worked on.
    pub logs: Option<PathBuf>,
}

#[derive(Clone, Default)]
struct Code {
    /// The last structure, as JSON text, and what Rust needs of it.
    structure: Option<String>,
    shape: Option<Shape>,
    /// The call graph for it, as the sidecar's whole message (JSON text).
    calls: Option<String>,
}

struct Shown {
    root: String,
    /// Changes with every new scanner process, so a stopped one's last words are ignored.
    generation: u64,
    sidecar: Option<Sidecar>,
    code: Code,
    // The watchers are held only to keep them running: dropping one stops it.
    #[allow(dead_code)]
    watch_root: Option<Watcher>,
    watch_folders: Vec<Watcher>,
    watching: Vec<String>,
    #[allow(dead_code)]
    watch_git: Vec<Watcher>,
    /// Its git folders, while it is in a repository, and what each part of git looked like when last told
    /// (`GitFolders::stamps`): a part that changed unseen by the watchers is news when the window gets the focus.
    git: Option<GitFolders>,
    stamps: [u64; 5],
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    watch_sidecar: Option<Watcher>,
}

/// Another tab: its last scan, and its scanner with the generation it was started as, while it still runs.
struct Kept {
    root: String,
    code: Code,
    scanner: Option<(Sidecar, u64)>,
}

struct Inner {
    settings: Settings,
    shown: Option<Shown>,
    /// The other tabs, the one shown last at the end.
    kept: Vec<Kept>,
    generation: u64,
}

pub struct Repos {
    context: Context,
    events: Arc<Events>,
    inner: Mutex<Inner>,
    /// The reader (sidecar/read.ts): one for the app, for the functions of a TypeScript-family file's other versions.
    reader: Mutex<Option<Sidecar>>,
    /// Per repository, the git parts Haunt announced after its own last write, and when: the .git watcher's news about
    /// the same write, a moment later, is not sent again (the window would read everything twice).
    announced: Mutex<HashMap<String, (Instant, Vec<String>)>>,
    /// The recent repositories as last read, by path (recent.rs), so the open screen does not run git again for each.
    recent: recent::Remembered,
    /// The repository folders as last searched (folders.rs), served at once and searched again in the background.
    folders: crate::folders::Searched,
    /// Some while the repository manager is open, with the repository folders' watcher (none while there is no folder
    /// to watch) (recent.rs).
    folder_watch: Mutex<Option<Option<Watcher>>>,
    /// The tabs and recent repositories whose folders were missing at the last look (missing.rs).
    missing: Mutex<Vec<String>>,
    me: Weak<Repos>,
}

impl Repos {
    pub fn new(context: Context, events: Arc<Events>) -> Arc<Repos> {
        let settings = Settings::load(&context.settings);
        Arc::new_cyclic(|me| Repos {
            context,
            events,
            inner: Mutex::new(Inner { settings, shown: None, kept: Vec::new(), generation: 0 }),
            reader: Mutex::new(None),
            announced: Mutex::default(),
            recent: recent::Remembered::default(),
            folders: crate::folders::Searched::default(),
            folder_watch: Mutex::new(None),
            missing: Mutex::default(),
            me: me.clone(),
        })
    }

    /// Shows the repository shown last time, if it can still be opened; when its folder is missing (a drive
    /// unplugged), the first tab whose folder is there.
    pub fn start(&self) {
        let mut inner = self.inner.lock();
        let root = inner
            .settings
            .root
            .clone()
            .filter(|root| paths::openable(Path::new(root)))
            .or_else(|| tabs::first_openable(&inner.settings.tabs));
        if let Some(root) = root {
            inner.settings.root = Some(root.clone());
            self.show(&mut inner, &root);
        }
    }

    /// The repository shown, the tabs, the recent ones, its trust and what it would run if trusted.
    pub fn info(&self) -> RootInfo {
        let mut info = {
            let inner = self.inner.lock();
            self.info_of(&inner)
        };
        // What the folder would run is found by starting git (its hooks): after the lock is let go, so no other call
        // waits for it.
        info.wants = info.root.as_deref().map(wants).unwrap_or_default();
        info
    }

    fn info_of(&self, inner: &Inner) -> RootInfo {
        let root = inner.shown.as_ref().map(|shown| shown.root.clone());
        let openable = |paths: &[String]| {
            paths.iter().filter(|path| paths::openable(Path::new(path))).cloned().collect::<Vec<_>>()
        };
        let tabs = openable(&inner.settings.tabs);
        let recent = openable(&inner.settings.recent);
        let mut missing: Vec<String> = Vec::new();
        for path in inner.settings.tabs.iter().chain(&inner.settings.recent) {
            if !tabs.contains(path) && !recent.contains(path) && !missing.contains(path) {
                missing.push(path.clone());
            }
        }
        RootInfo {
            tabs,
            recent,
            all_tabs: inner.settings.tabs.clone(),
            all_recent: inner.settings.recent.clone(),
            missing,
            trusted: root.as_deref().is_some_and(|root| inner.settings.trusts(root)),
            wants: Vec::new(),
            root,
            worktrees: inner
                .settings
                .tabs
                .iter()
                .filter(|tab| Path::new(tab).join(".git").is_file())
                .cloned()
                .collect(),
            profile: inner.settings.profile.clone(),
            profiles: inner.settings.profile_names(),
            appearance: inner.settings.appearance.clone(),
            tab_looks: inner
                .settings
                .tab_looks
                .iter()
                .filter(|(path, _)| inner.settings.tabs.contains(path))
                .map(|(path, look)| (path.clone(), look.clone()))
                .collect(),
        }
    }

    /// Refuses a call for a repository that is not the one shown.
    pub fn require(&self, repo: &str) -> Result<(), String> {
        let inner = self.inner.lock();
        self.check(&inner, repo)
    }

    fn check(&self, inner: &Inner, repo: &str) -> Result<(), String> {
        match &inner.shown {
            Some(shown) if shown.root == repo => Ok(()),
            _ => Err(said!("repos.notShown", "That repository is not the one shown.")),
        }
    }
}

/// The repository's git folder (also for worktrees and submodules, whose .git is a file pointing elsewhere).
fn git_dir(root: &str) -> Option<PathBuf> {
    let found = crate::git::cli::git(root, &["rev-parse", "--absolute-git-dir"]).ok()?;
    let found = found.trim();
    (!found.is_empty()).then(|| paths::plain(PathBuf::from(found)))
}
