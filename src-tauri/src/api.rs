//! What the window can ask: one list of commands, each written once. The `commands!` list below makes every entry a
//! Tauri command (typed for the window by specta, which writes `app/bindings.ts`) and, while Haunt is worked on, an
//! entry of the development bridge's `dispatch`, so the two cannot differ. Every command about a repository names it
//! (`repo`), and a call for one that is not shown is refused. Git's answers and the changes by function come from Rust
//! (`git/`, `code/`); the scan comes from the sidecar, as JSON passed through untouched (`Raw`).

mod bridge;
mod fetch;
mod scan;
#[cfg(test)]
mod tests;
mod versions;
mod writes;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;
use specta::Type;
use specta_typescript::Unknown;

use crate::code::changes::{Changes, RangeChanges};
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
use crate::repos::{AppFolder, AppSettings, Repos, RootInfo};
use crate::settings::PullChoice;
use crate::testrun::{TestRun, TestToRun};

use bridge::argument;
pub use fetch::{fetch_in_background, LastFetch};
use fetch::{fetches, record_fetch};
pub use scan::Source;

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
    /* Tabs, profiles and the repository manager (repos/) */

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

    /* Trust, the scan, the changes by function, and reading git (code/, git/) */

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
    /// Stops bisecting: back to the branch it started from.
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
    /// When the repository was last fetched (by you or in the background), and how it went; null before the first.
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
