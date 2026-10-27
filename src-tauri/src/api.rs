//! What the window can ask: one list of commands, each written once. The `commands!` list below makes every entry a
//! Tauri command (typed for the window by specta, which writes `app/bindings.ts`) and, while Haunt is worked on, an
//! entry of the development bridge's `dispatch`, so the two cannot differ. Every command about a repository names it
//! (`repo`), and a call for one that is not shown is refused. Git's answers and the changes by function come from Rust
//! (`git/`, `code/`); the scan comes from the sidecar, as JSON passed through untouched (`Raw`).

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;
use specta::Type;
use specta_typescript::Unknown;

use crate::code::changes::{Changes, RangeChanges, ScannedFile};
use crate::code::languages::{language_of, Language};
use crate::code::types::FunctionNode;
use crate::git::diff::Wanted;
use crate::git::local::LocalStatus;
use crate::git::types::{
    ChangedPath, CommitDetail, CommitInfo, FileBlame, FileDiffView, FileHistory, FunctionHistory, Hotspots, LogPage,
    LogQuery, Refs,
};
use crate::git::write::{
    Activity, CommitBox, ConflictFile, Discarded, DroppedStash, LastAction, LineRef, LinesAction, Operation, Outcome,
    PullMode, PullResult, PushResult, RebaseStep, ResetMode, Side, SwitchMode, Undo, Writer,
};
use crate::repos::{AppFolder, AppSettings, Reply, Repos, RootInfo};
use crate::settings::PullChoice;
use crate::testrun::{TestRun, TestToRun};

/// How long a call waits for the scanner (the first scan of a big repository with its call graph).
const WAIT: Duration = Duration::from_secs(120);

pub struct Api {
    pub repos: Arc<Repos>,
    /// What is missing on this machine (a JavaScript runtime, git), said once at start.
    pub problems: Vec<String>,
}

/// JSON from the sidecar, passed to the window as it is (the window knows it as the scan's types in shared/).
#[derive(Debug, Serialize, Type)]
#[serde(transparent)]
pub struct Raw(#[specta(type = Unknown)] Box<RawValue>);

impl Raw {
    fn from(text: String) -> Result<Raw, String> {
        RawValue::from_string(text).map(Raw).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Serialize, Type)]
pub struct Startup {
    pub problems: Vec<String>,
}

/// What the local changes compare the working tree with.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BaseChoice {
    /// Where the branch left main (or `branch`).
    MergeBase { branch: Option<String> },
    /// The last commit.
    Uncommitted,
    /// A branch, tag or commit.
    Ref { r#ref: String },
}

/// Lines of a scanned file as it is now.
#[derive(Debug, Serialize, Type)]
pub struct Source {
    pub start: u32,
    pub lines: Vec<String>,
    pub cut: bool,
}

/// Fetches the repository shown every so many minutes when Settings asks for it (Git: fetch in the background), so the
/// counts on Pull and Push stay current. Quietly: it asks nothing, waits for nothing and says nothing when it fails;
/// the window hears only that the refs changed. Each repository is fetched at most once per interval.
pub fn fetch_in_background(api: Arc<Api>) {
    std::thread::spawn(move || {
        let mut last: HashMap<String, std::time::Instant> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let (every, Some(root)) = api.repos.fetch_schedule() else { continue };
            let due = last.get(&root).is_none_or(|at| at.elapsed() >= Duration::from_secs(u64::from(every) * 60));
            if every == 0 || !due {
                continue;
            }
            last.insert(root.clone(), std::time::Instant::now());
            let Ok(trusted) = api.repos.trusted(&root) else { continue };
            let before = crate::git::upstream_tip(&root);
            let Some(result) = Writer::new(&root, trusted).fetch_in_background() else { continue };
            record_fetch(&root, &result, true);
            api.repos.announce_git(&root, &["refs"]);
            // New commits on the branch's upstream: the window says so (one click from seeing them).
            if let (Ok(()), Some(before), Some(after)) = (&result, before, crate::git::upstream_tip(&root)) {
                let commits = crate::git::count_between(&root, &before, &after);
                if commits > 0 {
                    api.repos.news(&root, serde_json::json!({ "type": "fetched", "commits": commits }));
                }
            }
        }
    });
}

/// When a repository was last fetched, and how it went.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LastFetch {
    /// Seconds since 1970.
    pub at: i64,
    pub ok: bool,
    /// What git said when it failed.
    pub message: Option<String>,
    /// A background fetch, not one you asked for.
    pub background: bool,
}

fn fetches() -> &'static parking_lot::Mutex<HashMap<String, LastFetch>> {
    static FETCHES: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, LastFetch>>> = std::sync::OnceLock::new();
    FETCHES.get_or_init(Default::default)
}

fn record_fetch(root: &str, result: &Result<(), String>, background: bool) {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    let entry = LastFetch { at, ok: result.is_ok(), message: result.as_ref().err().cloned(), background };
    fetches().lock().insert(root.to_owned(), entry);
}

fn wait(receiver: Receiver<Reply>) -> Reply {
    receiver
        .recv_timeout(WAIT)
        .unwrap_or_else(|_| Err(said!("api.scannerTooSlow", "The scanner took too long to answer.")))
}

impl Api {
    /// The files of the latest scan, by path, for the changes by function. A scan can be several megabytes of JSON:
    /// it is parsed once per scan (by repository and the scan's time) and kept.
    fn scanned_files(&self, repo: &str) -> Result<Arc<HashMap<String, ScannedFile>>, String> {
        #[derive(Deserialize)]
        struct Scan {
            files: Vec<ScannedFile>,
        }
        type Kept = Option<(String, String, Arc<HashMap<String, ScannedFile>>)>;
        static KEPT: std::sync::OnceLock<parking_lot::Mutex<Kept>> = std::sync::OnceLock::new();
        let text = self
            .repos
            .structure(repo)?
            .ok_or_else(|| said!("api.stillScanning", "Haunt is still scanning this repository."))?;
        // "scannedAt":"2026-10-25T…" sits near the start of the scan's JSON.
        let scanned_at = text
            .find("\"scannedAt\":\"")
            .and_then(|at| text[at + 13..].split('"').next())
            .unwrap_or_default()
            .to_owned();
        let kept = KEPT.get_or_init(Default::default);
        if let Some((known, at, files)) = kept.lock().as_ref() {
            if known == repo && !scanned_at.is_empty() && *at == scanned_at {
                return Ok(files.clone());
            }
        }
        let scan: Scan = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let files: Arc<HashMap<String, ScannedFile>> =
            Arc::new(scan.files.into_iter().map(|file| (file.path.clone(), file)).collect());
        *kept.lock() = Some((repo.to_owned(), scanned_at, files.clone()));
        Ok(files)
    }

    /// Runs a write in the shown repository `repo`, as its trust allows, and tells the window which parts of git changed.
    fn write<T>(
        &self,
        repo: &str,
        parts: &[&str],
        write: impl FnOnce(&Writer) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        crate::git::write::forget_last(repo);
        let result = write(&Writer::new(repo, trusted));
        // A failed write may have done part of its work (a save before a discard): the window reads again either way.
        self.repos.announce_git(repo, parts);
        result
    }

    /// A write that moves history, noted so the window can offer to undo it (git/write/last.rs): `title` names what
    /// it did, from its result.
    fn undoable<T>(
        &self,
        repo: &str,
        parts: &[&str],
        undo: Undo,
        title: impl FnOnce(&T) -> String,
        write: impl FnOnce(&Writer) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        let writer = Writer::new(repo, trusted);
        let before = writer.snapshot();
        crate::git::write::forget_last(repo);
        let result = write(&writer);
        if let Ok(value) = &result {
            self.noted(repo, &writer, title(value), before, undo);
        }
        self.repos.announce_git(repo, parts);
        result
    }

    /// Notes a write that went through and tells the window what Undo would do.
    fn noted(&self, repo: &str, writer: &Writer, title: String, before: crate::git::write::Snapshot, undo: Undo) {
        if let Some(last) = writer.record(title, before, undo) {
            self.repos.news(repo, serde_json::json!({ "type": "done", "title": last.title, "undo": last.undo }));
        }
    }

    /// A fetch, pull or push in `repo`: the window hears what `op` is doing (git's progress) and when it is done.
    fn network<T>(
        &self,
        repo: &str,
        op: &str,
        parts: &[&str],
        run: impl FnOnce(&Writer, &dyn Fn(&str)) -> Result<T, String>,
    ) -> Result<T, String> {
        let trusted = self.repos.trusted(repo)?;
        crate::git::write::forget_last(repo);
        self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "text": "" }));
        let progress =
            |text: &str| self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "text": text }));
        let result = run(&Writer::new(repo, trusted), &progress);
        self.repos.news(repo, serde_json::json!({ "type": "network", "op": op, "done": true }));
        self.repos.announce_git(repo, parts);
        result
    }

    /// A remote's write in a repository by its path; a tab that shows it reads its refs again.
    fn remote_by_path(&self, path: &str, write: impl FnOnce(&Writer) -> Result<(), String>) -> Result<(), String> {
        let root = crate::git::overview::repository(path)?;
        let result = write(&Writer::new(&root, false));
        self.repos.announce_git(&root, &["refs"]);
        result
    }

    /// A write that changes the lists of local changes: the lists after it.
    fn write_listed(
        &self,
        repo: &str,
        write: impl FnOnce(&Writer) -> Result<(), String>,
    ) -> Result<Option<LocalStatus>, String> {
        self.write(repo, &["index"], write)?;
        crate::git::local::status(repo)
    }

    /// The functions of a version of a file: the TypeScript family from the reader (sidecar/read.ts), the others read
    /// here; an error when the reader failed or took too long.
    fn try_read_version(&self, repo: &str, path: &str, text: &str) -> Result<Vec<FunctionNode>, String> {
        if language_of(path) != Some(Language::Typescript) {
            return Ok(crate::code::others::functions_of(repo, path, text));
        }
        self.repos
            .read_functions(repo, path, text)
            .and_then(wait)
            .and_then(|reply| serde_json::from_str(&reply).map_err(|error| error.to_string()))
    }

    /// The same, with no functions when it could not be read (a function's history, where a miss only ends the trail).
    fn read_version(&self, repo: &str, path: &str, text: &str) -> Vec<FunctionNode> {
        self.try_read_version(repo, path, text).unwrap_or_default()
    }

    /// Works out changes by function with `work`, then marks the files a version of which could not be read: their
    /// functions are left out (every one would otherwise look added or removed).
    fn by_function<T>(
        &self,
        repo: &str,
        work: impl FnOnce(&crate::code::changes::ReadVersion) -> Result<T, String>,
        files_of: impl FnOnce(&mut T) -> Option<&mut Vec<crate::code::changes::ChangedFile>>,
    ) -> Result<T, String> {
        let failed = std::cell::RefCell::new(std::collections::HashSet::new());
        let read = |path: &str, text: &str| match self.try_read_version(repo, path, text) {
            Ok(functions) => functions,
            Err(_) => {
                failed.borrow_mut().insert(path.to_owned());
                Vec::new()
            }
        };
        let mut result = work(&read)?;
        let failed = failed.into_inner();
        if !failed.is_empty() {
            if let Some(files) = files_of(&mut result) {
                for file in
                    files.iter_mut().filter(|file| failed.contains(&file.path) || failed.contains(&file.old_path))
                {
                    file.unread = true;
                    file.functions.clear();
                }
            }
        }
        Ok(result)
    }

    /// Lines `start` to `end` of a scanned file as it is now, for the function panel; at most 400.
    fn source(&self, repo: &str, path: &str, start: u32, end: u32) -> Result<Source, String> {
        const MAX_LINES: u32 = 400;
        if start < 1 || end < start {
            return Err(said!("api.functionNotScanned", "That function is not in the scan."));
        }
        let file = self.repos.scanned(repo, path)?;
        let text =
            std::fs::read_to_string(&file).map_err(|_| said!("api.fileUnread", "The file could not be read."))?;
        let all: Vec<&str> = text.lines().collect();
        let last = end.min(all.len() as u32).min(start + MAX_LINES - 1);
        let lines = all
            .get(start as usize - 1..last as usize)
            .map(|lines| lines.iter().map(|line| line.to_string()).collect())
            .unwrap_or_default();
        Ok(Source { start, lines, cut: last < end })
    }
}

/// An argument from the development bridge's JSON, by its name as the window sends it (camelCase, as Tauri expects).
fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    let mut camel = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            camel.extend(c.to_uppercase());
            upper = false;
        } else {
            camel.push(c);
        }
    }
    serde_json::from_value(args.get(&camel).cloned().unwrap_or(Value::Null))
        .map_err(|error| format!("\"{camel}\": {error}"))
}

/// Each entry becomes a Tauri command (on a worker thread: some wait for the scanner or git) and a `dispatch` case.
macro_rules! commands {
    ($( $(#[doc = $doc:literal])* fn $name:ident($api:ident $(, $arg:ident: $ty:ty)*) -> $ret:ty $body:block )*) => {
        $(
            $(#[doc = $doc])*
            #[tauri::command]
            #[specta::specta]
            #[allow(unused_braces)]
            // A command takes the window's arguments one by one: some take many.
            #[allow(clippy::too_many_arguments)]
            pub async fn $name(api: tauri::State<'_, Arc<Api>> $(, $arg: $ty)*) -> Result<$ret, String> {
                let $api = api.inner().clone();
                tauri::async_runtime::spawn_blocking(move || -> Result<$ret, String> { let $api: &Api = &$api; $body })
                    .await
                    .map_err(|error| error.to_string())?
            }
        )*

        /// The development bridge's way in: a command by name, its arguments as JSON, its answer as JSON text.
        #[cfg_attr(not(any(debug_assertions, test)), allow(dead_code))]
        #[allow(unused_braces)]
        pub fn dispatch(outer: &Api, name: &str, args: &Value) -> Result<String, String> {
            match name {
                $(
                    stringify!($name) => {
                        $(let $arg: $ty = argument(args, stringify!($arg))?;)*
                        let answer: Result<$ret, String> = { let $api: &Api = outer; $body };
                        let answer = answer?;
                        serde_json::to_string(&answer).map_err(|error| error.to_string())
                    }
                )*
                _ => Err(format!("Haunt has no command \"{name}\".")),
            }
        }

        /// Every command, for the Tauri builder and the generated bindings.
        pub fn collect() -> tauri_specta::Commands<tauri::Wry> {
            tauri_specta::collect_commands![$($name),*]
        }
    };
}

commands! {
    /// What is missing on this machine (a JavaScript runtime, git), said once at start.
    fn startup(api) -> Startup { Ok(Startup { problems: api.problems.clone() }) }
    fn info(api) -> RootInfo { Ok(api.repos.info()) }
    fn open(api, path: String) -> RootInfo { api.repos.open(&path) }
    fn close(api, path: String) -> RootInfo { Ok(api.repos.close(&path)) }
    /// The recent repositories, with each one's branch, uncommitted files and last commit.
    fn recent_details(api) -> Vec<crate::repos::RecentRepo> { Ok(api.repos.recent_details()) }
    /// Takes a folder off the recent list.
    fn forget_recent(api, path: String) -> RootInfo { Ok(api.repos.forget_recent(&path)) }
    /// The folders the repository manager looks through, each with the repositories in it.
    fn repository_folders(api) -> Vec<crate::folders::RepoFolder> { Ok(api.repos.repository_folders()) }
    fn add_repository_folder(api, path: String) -> Vec<crate::folders::RepoFolder> { api.repos.add_repository_folder(&path) }
    fn remove_repository_folder(api, path: String) -> Vec<crate::folders::RepoFolder> { Ok(api.repos.remove_repository_folder(&path)) }
    /// A repository at a glance, open in a tab or not: read only, nothing of its own runs.
    fn repository_overview(api, path: String) -> crate::git::overview::Overview { let _ = api; crate::git::overview::overview(&path) }
    /// Opens a page of one of a repository's remotes; any other address is refused.
    fn open_repository_page(api, path: String, remote: String, url: String) -> () { let _ = api; crate::git::overview::open_remote_page(&path, &remote, &url) }
    /// Who committed how much and when, and what the repository holds.
    fn repository_statistics(api, path: String) -> crate::git::overview::Statistics { let _ = api; crate::git::overview::statistics(&path) }
    /// Clones `url` into `parent`/`name`; the window hears git's progress as "clone" news. The new folder.
    fn clone_repository(api, url: String, parent: String, name: String) -> String {
        api.repos.news_for_all(serde_json::json!({ "type": "clone", "text": "" }));
        let progress = |text: &str| api.repos.news_for_all(serde_json::json!({ "type": "clone", "text": text }));
        let result = crate::git::write::clone(&url, &parent, &name, &progress);
        api.repos.news_for_all(serde_json::json!({ "type": "clone", "done": true }));
        result
    }
    /// Stops the clone going into `parent`.
    fn stop_clone(api, parent: String) -> bool {
        let _ = api;
        Ok(crate::git::write::cancel(&crate::paths::canonical(&parent).unwrap_or(parent)))
    }
    /// An empty repository in `parent`/`name`; its folder.
    fn init_repository(api, parent: String, name: String) -> String { let _ = api; crate::git::write::init(&parent, &name) }
    /// What a typed path is, before it is opened.
    fn check_folder(api, path: String) -> crate::repos::FolderCheck { let _ = api; Ok(crate::repos::check_folder(&path)) }
    /// Switches to another profile: its tabs, and the one it showed.
    fn switch_profile(api, name: String) -> RootInfo { api.repos.change_profiles(|settings| settings.switch_profile(&name)) }
    /// A new profile with no tabs, switched to.
    fn new_profile(api, name: String) -> RootInfo { api.repos.change_profiles(|settings| settings.new_profile(&name)) }
    fn rename_profile(api, from: String, to: String) -> RootInfo { api.repos.change_profiles(|settings| settings.rename_profile(&from, &to)) }
    /// Deletes a profile not in use; its folders stay where they are.
    fn delete_profile(api, name: String) -> RootInfo { api.repos.change_profiles(|settings| settings.delete_profile(&name)) }
    /// How the profile in use looks (light or dark, its colours, its corners).
    fn set_appearance(api, appearance: crate::settings::Appearance) -> RootInfo { api.repos.change_profiles(|settings| settings.set_appearance(appearance)) }
    /// Moves an open tab to another profile.
    fn move_tab(api, path: String, profile: String) -> RootInfo {
        api.repos.change_profiles(|settings| settings.move_tab(&path, &profile).map(|_| ()))
    }
    /// Pins a tab or not, and gives it a colour ("" for none).
    fn set_tab_look(api, path: String, look: crate::settings::TabLook) -> RootInfo {
        api.repos.change_profiles(|settings| settings.set_tab_look(&path, look))
    }
    /// Puts the tabs in a new order.
    fn reorder_tabs(api, tabs: Vec<String>) -> RootInfo { api.repos.reorder(&tabs) }
    /// Shows an open tab's folder in the file manager.
    fn reveal_tab(api, path: String) -> () { crate::editor::reveal(&api.repos.tab_folder(&path)?) }
    /// Opens a terminal in an open tab's folder.
    fn terminal_tab(api, path: String) -> () { crate::editor::terminal(&api.repos.tab_folder(&path)?) }
    fn trust(api, repo: String, trusted: bool) -> RootInfo { api.repos.set_trust(&repo, trusted) }
    fn rescan(api, repo: String) -> () { api.repos.rescan(&repo) }
    /// The latest structure (its calls come from `calls`), or null until the first scan is in.
    fn structure(api, repo: String) -> Option<Raw> { api.repos.structure(&repo)?.map(Raw::from).transpose() }
    /// The call graph of the latest structure, or null until it is worked out.
    fn calls(api, repo: String) -> Option<Raw> {
        let text = api.repos.calls(&repo)?;
        if text == "null" { Ok(None) } else { Raw::from(text).map(Some) }
    }
    /// What the working tree changes compared with `base`, function by function; null outside a git repository.
    fn changes(api, repo: String, base: BaseChoice) -> Option<Changes> {
        api.repos.require(&repo)?;
        if crate::git::diff::is_repository(&repo) {
            let files = api.scanned_files(&repo)?;
            let wanted = match &base {
                BaseChoice::MergeBase { branch } => Wanted::MergeBase(branch.as_deref()),
                BaseChoice::Uncommitted => Wanted::Uncommitted,
                BaseChoice::Ref { r#ref } => Wanted::Ref(r#ref),
            };
            api.by_function(&repo, |read| crate::code::changes::changes_of(&repo, wanted, &files, read), |changes| Some(&mut changes.files)).map(Some)
        } else {
            Ok(None)
        }
    }
    /// What `to` changes compared with `from` (null: its first commit), function by function.
    fn range(api, repo: String, from: Option<String>, to: String) -> RangeChanges {
        let files = api.scanned_files(&repo)?;
        api.by_function(&repo, |read| crate::code::changes::changes_between(&repo, from.as_deref(), &to, &files, read), |range| Some(&mut range.files))
    }
    /// What the staged changes change compared with the last commit, function by function (`to` is "index").
    fn staged_changes(api, repo: String) -> RangeChanges {
        let files = api.scanned_files(&repo)?;
        api.by_function(&repo, |read| crate::code::changes::staged_changes(&repo, &files, read), |range| Some(&mut range.files))
    }
    /// The commits `to` has that `from` has not (null: all of `to`'s), with the files each touched, for release notes.
    fn release_commits(api, repo: String, from: Option<String>, to: String) -> Vec<crate::git::ReleaseCommit> {
        api.repos.require(&repo)?;
        crate::git::release_commits(&repo, from.as_deref(), &to)
    }
    fn refs(api, repo: String) -> Option<Refs> { api.repos.require(&repo)?; crate::git::refs(&repo) }
    /// Files with changes not yet committed, as `git status` counts them; null when not in a git repository.
    fn local_changes(api, repo: String) -> Option<u32> { api.repos.require(&repo)?; crate::git::local_changes(&repo) }
    fn log(api, repo: String, query: LogQuery) -> LogPage { api.repos.require(&repo)?; crate::git::log(&repo, &query) }
    fn commit(api, repo: String, sha: String) -> CommitDetail { api.repos.require(&repo)?; crate::git::commit(&repo, &sha) }
    fn files(api, repo: String, from: Option<String>, to: String) -> Vec<ChangedPath> { api.repos.require(&repo)?; crate::git::files_between(&repo, from.as_deref(), &to) }
    fn diff(api, repo: String, from: Option<String>, to: String, path: String, old: Option<String>, context: u32, ignore_space: bool) -> FileDiffView {
        api.repos.require(&repo)?;
        let old = old.unwrap_or_else(|| path.clone());
        crate::git::file_diff_with(&repo, from.as_deref(), &to, &path, &old, context, ignore_space)
    }
    /// The local branches whose commits are all on the current branch, but the current one.
    fn merged_branches(api, repo: String) -> Vec<String> { api.repos.require(&repo)?; crate::git::merged_branches(&repo) }
    /// A function's history, carried on past the commits that moved it here from another file.
    fn history(api, repo: String, path: String, start: u32, end: u32) -> FunctionHistory {
        api.repos.require(&repo)?;
        let mut history = crate::git::function_history(&repo, &path, start, end);
        if let Ok(files) = api.scanned_files(&repo) {
            crate::code::changes::follow_moves(&repo, &mut history, &path, start, &files, &|path, text| api.read_version(&repo, path, text));
        }
        Ok(history)
    }
    /// The imports a commit (or a range) adds and removes, judged by the project's rules.
    fn change_imports(api, repo: String, from: Option<String>, to: String) -> crate::code::architecture::ImportChanges {
        crate::code::architecture::changes(&repo, from.as_deref(), &to, &|files| api.repos.ask_scanner(&repo, serde_json::json!({ "type": "imports", "files": files })))
    }
    /// The imports and rule breaks each of the last `limit` commits added and removed.
    fn architecture_history(api, repo: String, limit: u32) -> crate::code::architecture::ArchitectureHistory {
        crate::code::architecture::history(&repo, limit, &|files| api.repos.ask_scanner(&repo, serde_json::json!({ "type": "imports", "files": files })))
    }
    /// The last commit in which another file gained or lost a mention of `name`: about when an unused function lost its use.
    fn last_mention(api, repo: String, name: String, path: String) -> Option<CommitInfo> {
        api.repos.require(&repo)?;
        if crate::paths::inside(&repo, &path).is_none() {
            return Err(said!("api.pathOutside", "That path is not in this repository."));
        }
        crate::git::last_mention(&repo, &name, &path)
    }
    /// Who changed which file in the last `days` days.
    fn recent_authors(api, repo: String, days: u32) -> Vec<crate::git::knowledge::FileAuthor> { api.repos.require(&repo)?; crate::git::knowledge::recent_authors(&repo, days.clamp(1, 3650)) }
    /// Who wrote the lines of these files at HEAD (at most 300 files).
    fn blame_authors(api, repo: String, paths: Vec<String>) -> crate::git::knowledge::Blamed {
        api.repos.require(&repo)?;
        if paths.iter().any(|path| crate::paths::inside(&repo, path).is_none()) {
            return Err(said!("api.aPathOutside", "A path is not in this repository."));
        }
        Ok(crate::git::knowledge::blame_authors(&repo, &paths))
    }
    /// Who wrote the lines a change touches, as they were at `base`: the reviewers to suggest.
    fn reviewers(api, repo: String, base: String, spans: Vec<crate::git::knowledge::LineSpanAt>) -> crate::git::knowledge::Knowers {
        api.repos.require(&repo)?;
        if spans.iter().any(|span| crate::paths::inside(&repo, &span.path).is_none()) {
            return Err(said!("api.aPathOutside", "A path is not in this repository."));
        }
        crate::git::knowledge::reviewers(&repo, &base, &spans)
    }
    fn hotspots(api, repo: String, days: u32) -> Hotspots {
        api.repos.require(&repo)?;
        let days = days.clamp(1, 3650);
        Ok(Hotspots { days, files: crate::git::churn(&repo, days).unwrap_or_default() })
    }
    fn source(api, repo: String, path: String, start: u32, end: u32) -> Source { api.source(&repo, &path, start, end) }
    /// Opens a scanned file at a line in your editor.
    fn open_file(api, repo: String, editor: String, path: String, line: u32) -> () {
        let file = api.repos.scanned(&repo, &path)?;
        crate::editor::open(&editor, &repo, &file, line.max(1), 1)
    }
    /// The editors whose command line tool is on the PATH.
    fn editors(api) -> Vec<String> { let _ = api; Ok(crate::editor::installed()) }
    /// Saves an exported picture (the canvas, the layer map) where the save dialog said: base64 for a PNG.
    fn save_export(api, path: String, data: String, base64: bool) -> () { let _ = api; crate::export::save(&path, &data, base64) }

    /* Local changes: the lists and diffs, and writing (git/write) */

    /// The staged and unstaged files; null when not in a git repository.
    fn local_status(api, repo: String) -> Option<LocalStatus> { api.repos.require(&repo)?; crate::git::local::status(&repo) }
    /// One file's unstaged or staged diff (`old`: where a staged rename comes from).
    fn local_diff(api, repo: String, path: String, old: Option<String>, staged: bool) -> FileDiffView {
        api.repos.require(&repo)?;
        crate::git::local::local_diff(&repo, &path, old.as_deref(), staged)
    }
    // The staging and discarding writes answer with the lists as they are after it, for the window to show at once.
    fn stage(api, repo: String, paths: Vec<String>) -> Option<LocalStatus> { api.write_listed(&repo, |writer| writer.stage(&paths)) }
    fn unstage(api, repo: String, paths: Vec<String>) -> Option<LocalStatus> { api.write_listed(&repo, |writer| writer.unstage(&paths)) }
    /// Stages, unstages or discards a few lines of one file.
    fn lines(api, repo: String, path: String, lines: Vec<LineRef>, action: LinesAction) -> Option<LocalStatus> {
        api.write_listed(&repo, |writer| writer.lines(&path, &lines, action))
    }
    /// Throws away files' unstaged changes, after saving them (`restore` brings them back).
    fn discard(api, repo: String, paths: Vec<String>) -> Option<LocalStatus> { api.write_listed(&repo, |writer| writer.discard(&paths)) }
    /// The saved discards, newest first.
    fn discarded(api, repo: String) -> Vec<Discarded> { api.repos.require(&repo)?; Writer::new(&repo, false).discarded() }
    fn restore(api, repo: String, id: String) -> Option<LocalStatus> { api.write_listed(&repo, |writer| writer.restore(&id)) }
    /// Whether hooks and signing will run, and the last message (for amending).
    fn commit_box(api, repo: String) -> CommitBox { let trusted = api.repos.trusted(&repo)?; Writer::new(&repo, trusted).commit_box() }
    /// Commits what is staged, or amends the last commit; the new commit.
    fn commit_staged(api, repo: String, message: String, amend: bool, signoff: bool) -> String {
        let subject = message.lines().next().unwrap_or_default().trim().to_owned();
        let title = if amend {
            said!("api.amended", "Amended the last commit")
        } else {
            said!("api.committed", "Committed \"{subject}\"", subject = subject)
        };
        api.undoable(&repo, &["head", "index", "refs"], Undo::Reset("--soft"), |_| title, |writer| writer.commit(&message, amend, signoff))
    }
    /// What Haunt ran to write, newest first.
    fn activity(api, repo: String) -> Vec<Activity> { api.repos.require(&repo)?; Ok(crate::git::write::activity(&repo)) }

    /* Branches, stashes and tags (git/write/refs.rs) */

    /// Creates a branch at `at` (HEAD when null), and switches to it when asked.
    fn create_branch(api, repo: String, name: String, at: Option<String>, then_switch: bool) -> () {
        let undo = Undo::RemoveCreated { name: name.clone() };
        api.undoable(&repo, &["head", "refs", "index"], undo, |_| said!("api.created", "Created {name}", name = name), |writer| writer.create_branch(&name, at.as_deref(), then_switch))
    }
    /// Switches to a branch (a remote one through a local branch that tracks it), carrying or stashing local changes.
    fn switch_branch(api, repo: String, name: String, mode: SwitchMode) -> () {
        api.undoable(&repo, &["head", "refs", "index", "stash"], Undo::SwitchBack, |_| said!("api.switched", "Switched to {name}", name = name), |writer| writer.switch_branch(&name, mode))
    }
    /// Where a bisect stands (none going on: `active` is false).
    fn bisect_state(api, repo: String) -> crate::git::write::BisectState { api.repos.require(&repo)?; Ok(crate::git::write::bisect_state(&repo)) }
    /// Starts bisecting between a bad and a good commit.
    fn bisect_start(api, repo: String, bad: String, good: String) -> () { api.write(&repo, &["head", "refs", "index"], |writer| writer.bisect_start(&bad, &good)) }
    /// Marks the commit checked out as good, bad or skipped; the next one to test is checked out.
    fn bisect_mark(api, repo: String, mark: crate::git::write::BisectMark) -> () { api.write(&repo, &["head", "refs", "index"], |writer| writer.bisect_mark(mark)) }
    /// Tests another commit still in play instead.
    fn bisect_jump(api, repo: String, sha: String) -> () { api.write(&repo, &["head", "index"], |writer| writer.bisect_jump(&sha)) }
    /// Stops bisecting: back to the branch it started from.
    /// Saves `to` (with `from`: the commits after `from` up to `to`) as patch files in `folder`; returns their paths.
    fn save_patches(api, repo: String, from: Option<String>, to: String, folder: String) -> Vec<String> {
        api.write(&repo, &[], |writer| writer.save_patches(from.as_deref(), &to, &folder))
    }
    /// The patch files, and a commit on no branch with them applied to the last one, to see what they change.
    fn preview_patches(api, repo: String, files: Vec<String>) -> crate::git::write::PatchPreview {
        api.write(&repo, &[], |writer| writer.preview_patches(&files))
    }
    /// Applies patch files: as commits (`git am`) or as changes in the working tree only.
    fn apply_patches(api, repo: String, files: Vec<String>, commit: bool) -> () {
        api.write(&repo, &["head", "refs", "index"], |writer| writer.apply_patches(&files, commit))
    }
    /// The repository's submodules, with the commit each is checked out at.
    fn submodules(api, repo: String) -> Vec<crate::git::Submodule> { api.repos.require(&repo)?; crate::git::submodules(&repo) }
    /// Checks submodules out at the recorded commits, cloning missing ones (all when `paths` is empty); trusted only.
    fn update_submodules(api, repo: String, paths: Vec<String>) -> () {
        api.network(&repo, "submodules", &["head", "index"], |writer, progress| writer.update_submodules(&paths, progress))
    }
    /// Where HEAD, or a local branch, pointed before, newest first, with the commits only the reflog keeps marked.
    fn reflog(api, repo: String, reference: String) -> crate::git::reflog::Reflog { api.repos.require(&repo)?; crate::git::reflog::reflog(&repo, &reference) }
    /// A commit's signature, checked with your own gpg or ssh-keygen settings (never the repository's).
    fn commit_signature(api, repo: String, sha: String) -> crate::git::signature::Signature { api.repos.require(&repo)?; crate::git::signature::check(&repo, &sha) }
    /// Lines `start` to `end` of a file as it was at a commit (at most 200), for a preview; empty when it was not there.
    fn lines_at(api, repo: String, at: String, path: String, start: u32, end: u32) -> Vec<String> {
        api.repos.require(&repo)?;
        crate::git::cli_revision(&at)?;
        let path = crate::git::local::checked(&path)?;
        let text = crate::git::file_at(&repo, &at, path).unwrap_or_default();
        let start = start.max(1) as usize;
        let end = (end as usize).min(start + 199);
        Ok(text.lines().skip(start - 1).take(end.saturating_sub(start) + 1).map(str::to_owned).collect())
    }
    /// The room the history takes, its largest files, and the files that belong in Git LFS.
    fn health(api, repo: String) -> crate::git::Health { api.repos.require(&repo)?; crate::git::health(&repo) }
    /// The lines of the files at commit `at` that hold `pattern`, by file, from the opened folder down.
    fn grep(api, repo: String, at: String, pattern: String, mode: crate::git::grep::GrepMode, ignore_case: bool, path: Option<String>) -> crate::git::grep::GrepResult { api.repos.require(&repo)?; crate::git::grep::grep(&repo, &at, &pattern, mode, ignore_case, path.as_deref()) }
    /// What the folder would run once trusted (hooks, its architecture script, its TypeScript), with each file's text.
    fn wanted_texts(api, repo: String) -> Vec<crate::repos::WantedText> { api.repos.require(&repo)?; Ok(crate::repos::wanted_texts(&repo)) }
    /// The HTTP routes in the code: Hono and Express from the scanner, WordPress's REST routes read here.
    fn routes(api, repo: String) -> Vec<crate::code::routes::Route> {
        let files = api.scanned_files(&repo)?;
        let text = api.repos.ask_scanner(&repo, serde_json::json!({ "type": "routes" }))?;
        let mut routes: Vec<crate::code::routes::Route> = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let paths: Vec<String> = files.keys().cloned().collect();
        routes.extend(crate::code::routes::php_routes(&repo, &paths));
        routes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)));
        Ok(routes)
    }
    /// Sends a request to a dev server on this machine (Try it, from the Routes view); local addresses only.
    fn send_request(api, repo: String, request: crate::tryit::HttpRequest) -> crate::tryit::HttpResponse {
        api.repos.require(&repo)?;
        crate::tryit::send(&request)
    }
    /// The `.http` files at the top of the repository and in its `http/` folder, with their requests.
    fn http_files(api, repo: String) -> Vec<crate::tryit::HttpFile> { api.repos.require(&repo)?; Ok(crate::tryit::files(&repo)) }
    /// Saves a request in `http/<file>`, replacing the one of the same name there; returns the file's path.
    fn save_request(api, repo: String, file: String, variables: Vec<(String, String)>, request: crate::tryit::HttpRequest) -> String {
        api.repos.require(&repo)?;
        crate::tryit::save(&repo, &file, &variables, &request)
    }
    /// The remote's pull requests through its host's tool (gh, glab, tea), when switched on in Settings; empty otherwise.
    fn host_pulls(api, repo: String) -> crate::hosts::HostPulls {
        api.repos.require(&repo)?;
        let page = crate::git::web_remote(&repo);
        Ok(match page {
            Some(page) if api.repos.host_tools() => crate::hosts::host_pulls(&page),
            page => crate::hosts::HostPulls { tool: None, host: page.map(|page| page.host).unwrap_or_default(), pulls: Vec::new(), problem: None },
        })
    }
    /// What a new pull request for `branch` starts from: the remote's branches it could go into, and a title and
    /// description from its commits (into `base`, or the remote's default branch).
    fn pull_request_draft(api, repo: String, branch: String, base: Option<String>) -> crate::git::PullDraft {
        api.repos.require(&repo)?;
        crate::git::pull_draft(&repo, &branch, base.as_deref())
    }
    /// Makes a pull request (a merge request on GitLab) through the host's tool, on the remote the branch goes to.
    fn create_pull_request(api, repo: String, pull: crate::hosts::NewPull) -> crate::hosts::CreatedPull {
        api.repos.require(&repo)?;
        let draft = crate::git::pull_draft(&repo, &pull.branch, Some(&pull.base))?;
        let page = crate::git::remote_page(&repo, &draft.remote).ok_or_else(|| said!("api.noHost", "That remote has no host Haunt knows."))?;
        crate::hosts::create_pull(&page, &pull)
    }
    /// Opens a release page of Haunt itself in the browser (the update check's See it); nothing else.
    fn open_release(api, url: String) -> () {
        let _ = api;
        if !url.starts_with("https://github.com/razerghost/haunt/releases") {
            return Err(said!("api.notOurRelease", "Only Haunt's own release pages can be opened here."));
        }
        crate::editor::open_url(&url)
    }
    fn bisect_stop(api, repo: String) -> () { api.write(&repo, &["head", "refs", "index"], |writer| writer.bisect_stop()) }
    /// The repository's worktrees, its own folder first.
    fn worktrees(api, repo: String) -> Vec<crate::git::write::Worktree> { api.repos.require(&repo)?; crate::git::write::worktrees(&repo) }
    /// Checks a branch out in a new worktree (an existing branch, or a new one at `at`); returns its folder, to open.
    fn add_worktree(api, repo: String, path: String, branch: String, new_branch: bool, at: Option<String>) -> String {
        api.write(&repo, &["refs"], |writer| writer.add_worktree(&path, &branch, new_branch, at.as_deref()))
    }
    /// Removes a worktree's folder (its branch stays); one with changes not committed only with `force`.
    fn remove_worktree(api, repo: String, path: String, force: bool) -> () { api.write(&repo, &["refs"], |writer| writer.remove_worktree(&path, force)) }
    /// Forgets the worktrees whose folders are gone.
    fn prune_worktrees(api, repo: String) -> () { api.write(&repo, &["refs"], |writer| writer.prune_worktrees()) }
    fn rename_branch(api, repo: String, from: String, to: String) -> () {
        let undo = Undo::RenameBack { from: from.clone(), to: to.clone() };
        api.undoable(&repo, &["head", "refs"], undo, |_| said!("api.renamed", "Renamed {from} to {to}", from = from, to = to), |writer| writer.rename_branch(&from, &to))
    }
    /// How many commits only this branch has: what deleting it would lose.
    fn unmerged(api, repo: String, name: String) -> u32 { api.repos.require(&repo)?; Writer::new(&repo, false).unmerged(&name) }
    /// Deletes a branch; one with commits no other ref has only with `unmerged` (after asking).
    fn delete_branch(api, repo: String, name: String, unmerged: bool) -> () {
        api.repos.require(&repo)?;
        let sha = crate::git::cli::git(&repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")])
            .map(|sha| sha.trim().to_owned())
            .map_err(|_| said!("api.noBranch", "There is no branch called \"{name}\".", name = name))?;
        let undo = Undo::Recreate { name: name.clone(), sha };
        api.undoable(&repo, &["refs"], undo, |_| said!("api.deleted", "Deleted {name}", name = name), |writer| writer.delete_branch(&name, unmerged))
    }
    fn stash(api, repo: String, message: Option<String>, untracked: bool, paths: Vec<String>) -> () {
        api.write(&repo, &["stash", "index"], |writer| writer.stash(message.as_deref(), untracked, &paths))
    }
    /// Adds patterns to the repository's top .gitignore (anchored ones relative to the opened folder).
    fn ignore(api, repo: String, patterns: Vec<String>) -> () { api.write(&repo, &["index"], |writer| writer.ignore(&patterns)) }
    /// Applies a stash; `pop` also drops it when it applied cleanly.
    fn apply_stash(api, repo: String, index: u32, pop: bool) -> () {
        api.write(&repo, &["stash", "index"], |writer| writer.apply_stash(index, pop))
    }
    /// Drops a stash, kept for 30 days so it can be brought back.
    fn drop_stash(api, repo: String, index: u32) -> () { api.write(&repo, &["stash"], |writer| writer.drop_stash(index)) }
    fn dropped_stashes(api, repo: String) -> Vec<DroppedStash> { api.repos.require(&repo)?; Writer::new(&repo, false).dropped() }
    fn restore_stash(api, repo: String, id: String) -> () { api.write(&repo, &["stash"], |writer| writer.restore_stash(&id)) }
    /// Creates a tag at `at` (HEAD when null): annotated with a message, lightweight without.
    fn create_tag(api, repo: String, name: String, at: Option<String>, message: Option<String>) -> () {
        api.write(&repo, &["refs"], |writer| writer.create_tag(&name, at.as_deref(), message.as_deref()))
    }
    fn delete_tag(api, repo: String, name: String) -> () { api.write(&repo, &["refs"], |writer| writer.delete_tag(&name)) }

    /* Fetch, pull and push (git/write/network.rs) */

    /// Fetches every remote, pruning remote branches that are gone.
    fn fetch(api, repo: String) -> () {
        api.network(&repo, "fetch", &["refs"], |writer, progress| {
            let result = writer.fetch(progress);
            record_fetch(&repo, &result, false);
            result
        })
    }
    /// When the repository was last fetched (by you or in the background), and how it went; null before the first.
    /// Runs `tests` with the project's own test runner (testrun.rs); only in a trusted folder.
    fn run_tests(api, repo: String, tests: Vec<TestToRun>) -> TestRun {
        if !api.repos.trusted(&repo)? {
            return Err(said!("api.trustForTests", "Running tests runs the project's own code: trust this folder first."));
        }
        crate::testrun::run(&repo, &api.repos.node()?, &tests)
    }
    /// The icon sets downloaded so far (icons.rs); Lucide is bundled.
    fn icon_sets(api) -> Vec<String> { Ok(crate::icons::downloaded(&api.repos.config_dir())) }
    /// A downloaded icon set's icons, as an Iconify collection.
    fn icon_set(api, set: String) -> String { crate::icons::read(&api.repos.config_dir(), &set) }
    /// Checks and keeps an icon set the window downloaded.
    fn save_icon_set(api, set: String, json: String) -> () { crate::icons::save(&api.repos.config_dir(), &set, &json) }
    fn delete_icon_set(api, set: String) -> () { crate::icons::delete(&api.repos.config_dir(), &set) }
    /// A system notification, sent only when the window is not in front (alerts.rs).
    fn notify(api, title: String, body: String) -> () { let _ = api; crate::alerts::notify_if_away(&title, &body); Ok(()) }
    /// Stops the tests running now.
    fn stop_tests(api) -> () { let _ = api; crate::testrun::stop(); Ok(()) }
    fn last_fetch(api, repo: String) -> Option<LastFetch> { api.repos.require(&repo)?; Ok(fetches().lock().get(&repo).cloned()) }
    /// What a pull of the current branch would bring in; null when it tracks nothing.
    /// Fetches the remote's pull requests as refs: the newest 30, and `number` too when asked for.
    fn fetch_pull_requests(api, repo: String, remote: Option<String>, number: Option<u32>) -> crate::git::write::PullFetch {
        api.network(&repo, "fetch", &["refs"], |writer, progress| writer.fetch_pull_requests(remote.as_deref(), number, progress))
    }
    /// The pull requests fetched so far, newest number first.
    fn pull_requests(api, repo: String) -> Vec<crate::git::PullRequest> { api.repos.require(&repo)?; crate::git::pull_requests(&repo) }
    /// A pull request's commits after where it went apart from the main branch.
    fn pull_commits(api, repo: String, base: Option<String>, sha: String) -> Vec<CommitInfo> { api.repos.require(&repo)?; crate::git::pull_commits(&repo, base.as_deref(), &sha) }
    fn incoming(api, repo: String) -> Option<crate::git::Incoming> { api.repos.require(&repo)?; crate::git::incoming(&repo) }
    /// The project page of the current branch's remote on the web (GitHub, GitLab...); null when it has none.
    fn web_remote(api, repo: String) -> Option<crate::git::WebRemote> { api.repos.require(&repo)?; Ok(crate::git::web_remote(&repo)) }
    /// Opens a page of the repository's remote in the browser; any other address is refused.
    fn open_on_web(api, repo: String, url: String) -> () {
        api.repos.require(&repo)?;
        let page = crate::git::web_remote(&repo).ok_or_else(|| said!("api.noWebRemote", "This repository has no remote on the web."))?;
        if url != page.base && !url.starts_with(&format!("{}/", page.base)) {
            Err(said!("api.notItsPage", "Only pages of this repository's remote can be opened."))
        } else {
            crate::editor::open_url(&url)
        }
    }
    /// Pulls the current branch's upstream: fast-forward, merge or rebase.
    fn pull(api, repo: String, mode: PullMode) -> PullResult {
        let before = Writer::new(&repo, false).snapshot();
        let result = api.network(&repo, "pull", &["head", "refs", "index"], |writer, progress| writer.pull(mode, progress))?;
        api.noted(&repo, &Writer::new(&repo, false), said!("api.pulled", "Pulled"), before, Undo::Reset("--keep"));
        Ok(result)
    }
    /// Pushes the current branch (to `remote` the first time, when there are several); `force` overwrites with a lease.
    fn push(api, repo: String, remote: Option<String>, force: bool) -> PushResult {
        api.network(&repo, "push", &["refs"], |writer, progress| writer.push(remote.as_deref(), force, progress))
    }
    fn push_tag(api, repo: String, name: String, remote: Option<String>) -> PushResult {
        api.network(&repo, "push", &["refs"], |writer, progress| writer.push_tag(&name, remote.as_deref(), progress))
    }
    /// Stops the fetch, pull or push running in `repo`.
    /// Remotes of a repository by its path, open in a tab or not (the repository manager's): config only, so nothing
    /// of the repository runs and its trust does not matter.
    fn repository_add_remote(api, path: String, name: String, url: String) -> () {
        api.remote_by_path(&path, |writer| writer.add_remote(&name, &url))
    }
    fn repository_rename_remote(api, path: String, from: String, to: String) -> () {
        api.remote_by_path(&path, |writer| writer.rename_remote(&from, &to))
    }
    fn repository_remove_remote(api, path: String, name: String) -> () { api.remote_by_path(&path, |writer| writer.remove_remote(&name)) }
    fn repository_set_remote_url(api, path: String, name: String, url: String) -> () {
        api.remote_by_path(&path, |writer| writer.set_remote_url(&name, &url))
    }
    fn add_remote(api, repo: String, name: String, url: String) -> () { api.write(&repo, &["refs"], |writer| writer.add_remote(&name, &url)) }
    /// Renames a remote, with its remote branches.
    fn rename_remote(api, repo: String, from: String, to: String) -> () { api.write(&repo, &["refs"], |writer| writer.rename_remote(&from, &to)) }
    /// Removes a remote and its remote branches.
    fn remove_remote(api, repo: String, name: String) -> () { api.write(&repo, &["refs"], |writer| writer.remove_remote(&name)) }
    fn set_remote_url(api, repo: String, name: String, url: String) -> () { api.write(&repo, &["refs"], |writer| writer.set_remote_url(&name, &url)) }
    fn stop_network(api, repo: String) -> bool { api.repos.require(&repo)?; Ok(crate::git::write::cancel(&repo)) }
    /* Undoing, merging and conflicts (git/write/undo.rs, git/write/conflicts.rs) */

    /// A new commit that takes `at`'s changes back.
    fn revert(api, repo: String, at: String) -> Outcome {
        let title = said!("api.reverted", "Reverted {at}", at = short(&at));
        api.undoable(&repo, &["head", "refs", "index"], Undo::Reset("--keep"), |_| title, |writer| writer.revert(&at))
    }
    /// `at`'s changes again, as a new commit on the current branch.
    fn cherry_pick(api, repo: String, at: String) -> Outcome {
        let title = said!("api.cherryPicked", "Cherry-picked {at}", at = short(&at));
        api.undoable(&repo, &["head", "refs", "index"], Undo::Reset("--keep"), |_| title, |writer| writer.cherry_pick(&at))
    }
    /// Moves the current branch to `at`: soft, mixed or hard (which saves your local changes first).
    fn reset(api, repo: String, at: String, mode: ResetMode) -> () {
        let undo = Undo::Reset(match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--keep",
        });
        let title = said!("api.resetTo", "Reset to {at}", at = short(&at));
        api.undoable(&repo, &["head", "refs", "index"], undo, |_| title, |writer| writer.reset(&at, mode))
    }
    /// Takes the last commit back; its changes are staged again.
    fn undo_commit(api, repo: String) -> () {
        api.undoable(&repo, &["head", "refs", "index"], Undo::Reset("--soft"), |_| said!("api.tookBack", "Took back the last commit"), |writer| writer.undo_commit())
    }
    /// The remote branches that have `at`.
    fn on_remotes(api, repo: String, at: String) -> Vec<String> { api.repos.require(&repo)?; Writer::new(&repo, false).on_remotes(&at) }
    /// Merges a branch, remote branch or tag into the current branch.
    fn merge(api, repo: String, name: String) -> Outcome {
        api.undoable(&repo, &["head", "refs", "index"], Undo::Reset("--keep"), |_| said!("api.merged", "Merged {name}", name = name), |writer| writer.merge(&name))
    }
    /// The last write that can be undone (a commit, merge, reset, branch switch…), and what Undo does; null when none.
    fn last_action(api, repo: String) -> Option<LastAction> { api.repos.require(&repo)?; Ok(Writer::new(&repo, false).last_action()) }
    /// Undoes it; returns what was undone.
    fn undo_last(api, repo: String) -> String {
        // Not through `write`, which forgets the last action first.
        let trusted = api.repos.trusted(&repo)?;
        let result = Writer::new(&repo, trusted).undo_last();
        api.repos.announce_git(&repo, &["head", "refs", "index"]);
        result
    }
    /// The merge, revert, cherry-pick or rebase going on, with its conflicted files; null when none.
    fn operation(api, repo: String) -> Option<Operation> { api.repos.require(&repo)?; Writer::new(&repo, false).operation() }
    /// A conflicted file's base, yours, theirs and the file as it is now.
    fn conflict(api, repo: String, path: String) -> ConflictFile { api.repos.require(&repo)?; Writer::new(&repo, false).conflict(&path) }
    /// Resolves a file by taking one side whole.
    fn take_side(api, repo: String, path: String, side: Side) -> () { api.write(&repo, &["index"], |writer| writer.take(&path, side)) }
    /// Resolves a file with the text written in the window (as it is on disk when null).
    fn resolve(api, repo: String, path: String, text: Option<String>) -> () {
        api.write(&repo, &["index"], |writer| writer.resolve(&path, text.as_deref()))
    }
    /// Finishes the merge, revert, cherry-pick or rebase once nothing conflicts.
    fn continue_operation(api, repo: String) -> Outcome { api.write(&repo, &["head", "refs", "index"], |writer| writer.proceed()) }
    /// Stops it and puts everything back as it was before.
    fn abort_operation(api, repo: String) -> () { api.write(&repo, &["head", "refs", "index"], |writer| writer.abort()) }

    /* History in depth (git/history.rs, git/write/rebase.rs) */

    /// A page of a file's history, newest first, followed across renames.
    fn file_history(api, repo: String, path: String, skip: u32, limit: u32) -> FileHistory {
        api.repos.require(&repo)?;
        crate::git::file_history(&repo, &path, skip, limit)
    }
    /// Who changed each line of a file last: in the working tree, or at commit `at`.
    fn file_blame(api, repo: String, path: String, at: Option<String>) -> FileBlame {
        api.repos.require(&repo)?;
        crate::git::file_blame(&repo, &path, at.as_deref())
    }
    /// The commits of the current branch after `onto`, oldest first, for an interactive rebase.
    fn rebase_plan(api, repo: String, onto: String) -> Vec<CommitInfo> { api.repos.require(&repo)?; Writer::new(&repo, false).rebase_plan(&onto) }
    /// Rebases the current branch onto `onto` with the commits in the new order, each kept, reworded, squashed or dropped.
    fn rebase(api, repo: String, onto: String, steps: Vec<RebaseStep>) -> Outcome {
        api.undoable(&repo, &["head", "refs", "index"], Undo::Reset("--keep"), |_| said!("api.rebased", "Rebased onto {onto}", onto = onto), |writer| writer.rebase(&onto, &steps))
    }

    /* Settings */

    /// The git settings, the trusted folders and Haunt's own folders.
    fn app_settings(api) -> AppSettings { Ok(api.repos.app_settings()) }
    /// What a pull does when the branches went apart, and how often to fetch in the background (0, 5, 15 or 30 minutes).
    fn set_git_settings(api, pull: PullChoice, fetch_every: u32) -> AppSettings { api.repos.set_git(pull, fetch_every) }
    /// Pull request status through the host's tool, and the update check: both off by default.
    fn set_online(api, host_tools: bool, update_check: bool) -> AppSettings { Ok(api.repos.set_online(host_tools, update_check)) }
    /// Stops trusting a folder, shown or not.
    fn forget_trust(api, path: String) -> AppSettings { Ok(api.repos.forget_trust(&path)) }
    /// Removes the kept call graphs; how many there were.
    fn clear_call_graphs(api) -> u32 { api.repos.clear_call_graphs() }
    /// Opens one of Haunt's own folders in the file manager.
    fn open_folder(api, which: AppFolder) -> () {
        let folder = api.repos.folder(which).ok_or_else(|| said!("api.noSuchFolder", "Haunt keeps no such folder here."))?;
        crate::editor::reveal(&folder)
    }

    /// The answer to one of git's questions (the news `ask`): the text, or null to cancel.
    fn answer_question(api, id: u32, answer: Option<String>) -> () { let _ = api; crate::askpass::answer(id.into(), answer); Ok(()) }
}

/// A commit for a title: its short sha, or the name it was given (a branch, a tag).
fn short(at: &str) -> String {
    if at.len() == 40 && at.chars().all(|c| c.is_ascii_hexdigit()) {
        at[..7].to_owned()
    } else {
        at.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Events;
    use crate::repos::Context;
    use serde_json::json;
    use std::path::PathBuf;

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
        let call = |name: &str, args: Value| {
            dispatch(&api, name, &args).map(|text| serde_json::from_str::<Value>(&text).unwrap())
        };

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
        let files: Vec<String> = structure["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["path"].as_str().unwrap().to_owned())
            .collect();
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
        let call = |name: &str, args: Value| {
            dispatch(&api, name, &args).map(|text| serde_json::from_str::<Value>(&text).unwrap())
        };
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
        let test = laravel["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "tests/Feature/OrderTest.php")
            .unwrap();
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
        let typescript = |path: &str| {
            crate::code::languages::language_of(path) == Some(crate::code::languages::Language::Typescript)
        };
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
                specta_typescript::Typescript::default().header(
                    "// Written by the Rust tests (src-tauri/src/api.rs) from Haunt's commands: do not edit.\n",
                ),
                path,
            )
            .expect("the bindings could not be written");
        let written = std::fs::read_to_string(path).unwrap();
        for name in ["log", "commit", "openFile", "structure"] {
            assert!(written.contains(&format!("	{name}: (")), "{name} is missing from the bindings");
        }
    }
}
