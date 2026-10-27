//! The repositories: the open tabs, the one shown with its scanner process and watchers, trust, and the last scan of a
//! few other tabs, kept in memory so going back to one shows it at once while it is scanned again. The last few of those
//! keep their scanner running too: going back asks it to scan again, which reads only what changed, instead of starting
//! a new one that reads everything. Every call names its repository; a call for one that is not shown is refused, so an
//! answer can never land in the wrong tab.

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
use crate::watch::Watcher;

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
    pub tabs: Vec<String>,
    pub recent: Vec<String>,
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
            me: me.clone(),
        })
    }

    /// Shows the repository shown last time, if it can still be opened.
    pub fn start(&self) {
        let root = self.inner.lock().settings.root.clone();
        if let Some(root) = root.filter(|root| paths::openable(Path::new(root))) {
            let mut inner = self.inner.lock();
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
        RootInfo {
            tabs: openable(&inner.settings.tabs),
            recent: openable(&inner.settings.recent),
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
