//! Git's reads, on repositories the tests build: the log, refs, a commit and its files, diffs, history and churn, and
//! the safety rule that a repository's own diff driver, textconv and filters never run.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::types::{DecorationKind, LineKind, LogQuery, MergeFilter, PathStatus};
use super::*;

const A1: &str = "export function keep() { return 1 }\nexport function edit() {\n  return 1\n}\nexport function drop() { return 3 }\nexport class Order {\n  total() { return 0 }\n}\n";
const A2: &str = "export function keep() { return 1 }\nexport function edit() {\n  return 2\n}\nexport class Order {\n  total() { return 0 }\n}\nexport function fresh() { return keep() }\n";

pub(crate) struct Repo {
    _dir: tempfile::TempDir,
    pub(crate) root: String,
    /// Each commit a minute after the last, from yesterday: commits made in one second would tie in blame and log.
    clock: Cell<u64>,
}

impl Repo {
    pub(crate) fn new() -> Repo {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::plain(fs::canonicalize(dir.path()).unwrap()).to_string_lossy().into_owned();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let repo = Repo { _dir: dir, root, clock: Cell::new(now - 86_400) };
        repo.sh(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("core.autocrlf", "false"),
            ("commit.gpgsign", "false"),
        ] {
            repo.sh(&["config", key, value]);
        }
        repo
    }

    pub(crate) fn sh(&self, args: &[&str]) -> String {
        self.clock.set(self.clock.get() + 60);
        let date = format!("@{} +0000", self.clock.get());
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub(crate) fn write(&self, path: &str, text: &str) {
        fs::write(Path::new(&self.root).join(path), text).unwrap();
    }

    pub(crate) fn path(&self, path: &str) -> PathBuf {
        Path::new(&self.root).join(path)
    }
}

/// main: a.ts, old.ts, gone.ts, other.ts; feature (two commits): a.ts edited, old.ts renamed; main moves on.
fn feature_branch() -> Repo {
    let r = Repo::new();
    r.write("a.ts", A1);
    r.write("old.ts", "export function moved() {\n  return \"a\"\n}\nexport function same() { return 1 }\n");
    r.write("gone.ts", "export function lost() { return 1 }\n");
    r.write("other.ts", "export function elsewhere() { return 1 }\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r.sh(&["switch", "-q", "-c", "feature"]);
    r.write("a.ts", A2);
    r.sh(&["commit", "-q", "-am", "Edit a"]);
    r.sh(&["mv", "old.ts", "renamed.ts"]);
    r.write("renamed.ts", "export function moved() {\n  return \"b\"\n}\nexport function same() { return 1 }\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Rename old"]);
    r.sh(&["switch", "-q", "main"]);
    r.write("other.ts", "export function elsewhere() { return 2 }\n");
    r.sh(&["commit", "-q", "-am", "Main moves on"]);
    r.sh(&["switch", "-q", "feature"]);
    // Uncommitted: a new file git does not know yet, and a deleted one.
    r.write("new.ts", "export function brandNew() { return 1 }\n");
    fs::remove_file(r.path("gone.ts")).unwrap();
    r
}

fn subjects(page: &types::LogPage) -> Vec<&str> {
    page.commits.iter().map(|commit| commit.subject.as_str()).collect()
}

fn query(from: &[&str], skip: u32, limit: u32) -> LogQuery {
    LogQuery {
        from: (!from.is_empty()).then(|| from.iter().map(|name| name.to_string()).collect()),
        skip: Some(skip),
        limit: Some(limit),
        ..LogQuery::default()
    }
}

#[test]
fn a_files_history_follows_it_across_a_rename_and_blame_names_each_line() {
    let r = Repo::new();
    r.write("old.txt", "one\ntwo\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Add old"]);
    r.sh(&["mv", "old.txt", "new.txt"]);
    r.sh(&["commit", "-q", "-m", "Rename"]);
    r.write("new.txt", "one\nTWO\nthree\n");
    r.sh(&["commit", "-q", "-am", "Change"]);
    r.write("other.txt", "x\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Other file"]);

    let history = file_history(&r.root, "new.txt", 0, 50).unwrap();
    let subjects: Vec<&str> = history.versions.iter().map(|version| version.commit.subject.as_str()).collect();
    assert_eq!(subjects, ["Change", "Rename", "Add old"], "other files' commits are not in it");
    assert!(matches!(history.versions[1].status, PathStatus::Renamed));
    assert_eq!((history.versions[1].old_path.as_str(), history.versions[1].path.as_str()), ("old.txt", "new.txt"));
    assert_eq!(history.versions[2].path, "old.txt");
    assert!(history.versions[2].parent.is_none(), "the first commit has no parent");
    assert!(!history.more);
    let page = file_history(&r.root, "new.txt", 1, 1).unwrap();
    assert_eq!(page.versions[0].commit.subject, "Rename");
    assert!(page.more);

    // Blame: line by line, with a change not committed yet as zeros.
    r.write("new.txt", "one\nTWO\nthree\nfour\n");
    let blame = file_blame(&r.root, "new.txt", None).unwrap();
    assert_eq!(blame.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>(), ["one", "TWO", "three", "four"]);
    let subject =
        |sha: &str| blame.commits.iter().find(|commit| commit.sha == sha).map(|commit| commit.subject.clone());
    assert_eq!(subject(&blame.lines[0].sha).as_deref(), Some("Add old"));
    assert_eq!(subject(&blame.lines[1].sha).as_deref(), Some("Change"));
    assert!(blame.lines[3].sha.chars().all(|c| c == '0'));
    // At a commit: the file as it was then.
    let then = file_blame(&r.root, "new.txt", Some("HEAD~2")).unwrap();
    assert_eq!(then.lines.len(), 2);
    assert!(file_blame(&r.root, "new.txt", Some("--all")).is_err());
}

#[test]
fn lists_the_commits_with_their_parents_and_refs_filtered_and_in_pages() {
    let r = feature_branch();
    r.sh(&["merge", "-q", "--no-ff", "main", "-m", "Merge main into feature"]);
    r.sh(&["tag", "v1", "main"]);

    let all = log(&r.root, &LogQuery::default()).unwrap();
    assert_eq!(subjects(&all), ["Merge main into feature", "Main moves on", "Rename old", "Edit a", "First"]);
    assert!(!all.more);
    let merge = &all.commits[0];
    assert_eq!(merge.parents.len(), 2);
    assert_eq!(
        merge.refs.iter().map(|d| (d.kind, d.name.as_str())).collect::<Vec<_>>(),
        [(DecorationKind::Head, "HEAD"), (DecorationKind::Branch, "feature")]
    );
    let mut main_refs: Vec<String> = all.commits[1].refs.iter().map(|d| format!("{:?}:{}", d.kind, d.name)).collect();
    main_refs.sort();
    assert_eq!(main_refs, ["Branch:main", "Tag:v1"]);
    assert_eq!(all.commits[1].author, "Test");

    let paged = log(&r.root, &query(&[], 0, 2)).unwrap();
    assert_eq!((paged.commits.len(), paged.more), (2, true));
    assert_eq!(subjects(&log(&r.root, &query(&[], 2, 10)).unwrap()), ["Rename old", "Edit a", "First"]);
    assert_eq!(subjects(&log(&r.root, &query(&["main"], 0, 300)).unwrap()), ["Main moves on", "First"]);
    let by_message = LogQuery { message: Some("RENAME".into()), ..LogQuery::default() };
    assert_eq!(subjects(&log(&r.root, &by_message).unwrap()), ["Rename old"]);
    let by_author = LogQuery { author: Some("nobody".into()), ..LogQuery::default() };
    // Only the commits that added or removed a text.
    let added = log(&r.root, &LogQuery { changed: Some("return 2".into()), ..LogQuery::default() }).unwrap();
    assert!(!added.commits.is_empty());
    assert!(log(&r.root, &LogQuery { changed: Some("no such text anywhere".into()), ..LogQuery::default() })
        .unwrap()
        .commits
        .is_empty());
    // Merges only, or none: together they are every commit.
    let only = log(&r.root, &LogQuery { merges: Some(MergeFilter::Only), ..LogQuery::default() }).unwrap();
    let none = log(&r.root, &LogQuery { merges: Some(MergeFilter::None), ..LogQuery::default() }).unwrap();
    let every = log(&r.root, &LogQuery::default()).unwrap();
    assert_eq!(only.commits.len() + none.commits.len(), every.commits.len());
    assert!(only.commits.iter().all(|commit| commit.parents.len() > 1));
    assert!(none.commits.iter().all(|commit| commit.parents.len() <= 1));
    // By date: the same commits, children still before their parents.
    let dated = log(&r.root, &LogQuery { by_date: Some(true), ..LogQuery::default() }).unwrap();
    let topo = log(&r.root, &LogQuery::default()).unwrap();
    assert_eq!(dated.commits.len(), topo.commits.len());
    assert!(log(&r.root, &by_author).unwrap().commits.is_empty());
    // Filtered by a path, a commit's parents are the earlier commits that touch it too.
    let other = log(&r.root, &LogQuery { path: Some("other.ts".into()), ..LogQuery::default() }).unwrap();
    assert_eq!(subjects(&other), ["Main moves on", "First"]);
    assert_eq!(other.commits[0].parents, [other.commits[1].sha.clone()]);
    assert!(log(&r.root, &query(&["--output=x"], 0, 10)).unwrap_err().contains("is not a branch, tag or commit"));
}

/// Pages read on from a `git log` kept running (log.rs): the same commits as one long page, a page asked out of turn
/// read afresh, and the first page always afresh, with what was committed since.
#[test]
fn reads_pages_on_from_where_the_last_one_stopped() {
    let r = feature_branch();
    let whole = subjects(&log(&r.root, &query(&[], 0, 300)).unwrap()).iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut paged = Vec::new();
    for skip in 0..whole.len() as u32 {
        let page = log(&r.root, &query(&[], skip, 1)).unwrap();
        assert_eq!(page.more, (skip as usize) < whole.len() - 1);
        paged.extend(subjects(&page).iter().map(|s| s.to_string()));
    }
    assert_eq!(paged, whole);
    // Out of turn: page 3 straight after page 1.
    log(&r.root, &query(&[], 0, 1)).unwrap();
    assert_eq!(subjects(&log(&r.root, &query(&[], 3, 1)).unwrap()), [whole[3].as_str()]);
    // A commit made since shows on the first page.
    log(&r.root, &query(&[], 0, 1)).unwrap();
    r.write(
        "late.txt", "late
",
    );
    r.sh(&["add", "late.txt"]);
    r.sh(&["commit", "-q", "-m", "Late"]);
    assert_eq!(subjects(&log(&r.root, &query(&[], 0, 1)).unwrap()), ["Late"]);
}

#[test]
fn reads_a_commit_with_its_files_and_one_files_diff() {
    let r = feature_branch();
    let page = log(&r.root, &query(&["feature"], 0, 300)).unwrap();
    let (rename, edit, first) = (&page.commits[0], &page.commits[1], &page.commits[2]);

    let renamed = commit(&r.root, &rename.sha).unwrap();
    assert_eq!(renamed.subject, "Rename old");
    assert_eq!(renamed.parents, std::slice::from_ref(&edit.sha));
    assert_eq!(renamed.files.len(), 1);
    let file = &renamed.files[0];
    assert_eq!(
        (file.path.as_str(), file.old_path.as_str(), file.status, file.added, file.removed),
        ("renamed.ts", "old.ts", PathStatus::Renamed, 1, 1)
    );
    assert!(renamed.refs.iter().any(|d| d.kind == DecorationKind::Branch && d.name == "feature"));

    // The first commit: every file is added.
    let root = commit(&r.root, &first.sha).unwrap();
    assert!(root.parents.is_empty());
    let mut added: Vec<String> = root.files.iter().map(|f| format!("{:?}:{}", f.status, f.path)).collect();
    added.sort();
    assert_eq!(added, ["Added:a.ts", "Added:gone.ts", "Added:old.ts", "Added:other.ts"]);

    // Between two commits: what the branch did since main's first commit.
    let between: Vec<String> = files_between(&r.root, Some(&first.sha), &rename.sha)
        .unwrap()
        .iter()
        .map(|f| format!("{:?}:{}", f.status, f.path))
        .collect();
    assert_eq!(between, ["Modified:a.ts", "Renamed:renamed.ts"]);

    let diff = file_diff(&r.root, Some(&first.sha), &edit.sha, "a.ts", "a.ts", 3).unwrap();
    assert!(!diff.binary);
    let lines: Vec<_> = diff.hunks.iter().flat_map(|hunk| hunk.lines.iter()).collect();
    assert!(lines.iter().any(|line| line.kind == LineKind::Del && line.text == "  return 1" && line.old == Some(3)));
    assert!(lines.iter().any(|line| line.kind == LineKind::Add && line.text == "  return 2" && line.new == Some(3)));
    assert!(lines.iter().any(|line| line.kind == LineKind::Del && line.text.starts_with("export function drop")));
    // More context: the whole file in one hunk.
    let whole = file_diff(&r.root, Some(&first.sha), &edit.sha, "a.ts", "a.ts", 1000).unwrap();
    assert_eq!(whole.hunks.len(), 1);
    assert_eq!(whole.hunks[0].lines.iter().filter(|line| line.kind != LineKind::Del).count(), A2.lines().count());
    // A renamed file's diff follows it from its old name; a new file is all added.
    let moved = file_diff(&r.root, Some(&edit.sha), &rename.sha, "renamed.ts", "old.ts", 3).unwrap();
    let changed: Vec<&str> = moved
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.kind != LineKind::Context)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(changed, ["  return \"a\"", "  return \"b\""]);
    let created = file_diff(&r.root, None, &first.sha, "gone.ts", "gone.ts", 3).unwrap();
    assert_eq!((created.hunks[0].old_start, created.hunks[0].old_count, created.hunks[0].new_start), (0, 0, 1));
    assert!(commit(&r.root, "no-such-commit").is_err());
}

#[test]
fn reads_the_branches_with_their_upstreams_tags_stashes_and_local_changes() {
    let r = feature_branch();
    let remote = tempfile::tempdir().unwrap();
    let remote_path = remote.path().to_string_lossy().into_owned();
    assert!(Command::new("git").args(["init", "-q", "--bare", &remote_path]).status().unwrap().success());
    r.sh(&["remote", "add", "origin", &remote_path]);
    r.sh(&["push", "-q", "origin", "main~1:refs/heads/main", "feature:refs/heads/old"]);
    r.sh(&["branch", "-q", "--set-upstream-to=origin/main", "main"]);
    r.sh(&["branch", "-q", "old", "feature"]);
    r.sh(&["branch", "-q", "--set-upstream-to=origin/old", "old"]);
    r.sh(&["push", "-q", "origin", "--delete", "old"]);
    r.sh(&["tag", "v1", "main~1"]);
    r.sh(&["tag", "-a", "v2", "-m", "Second", "main"]);
    r.write("a.ts", &format!("{A2}// stashed\n"));
    r.sh(&["stash", "push", "-q", "-m", "Half done", "--", "a.ts"]);

    let found = refs(&r.root).unwrap().unwrap();
    assert_eq!(found.branch.as_deref(), Some("feature"));
    let branch = |name: &str| found.branches.iter().find(|branch| branch.name == name).unwrap();
    assert!(branch("feature").current);
    assert_eq!(branch("feature").upstream, None);
    assert_eq!(branch("main").upstream.as_deref(), Some("origin/main"));
    assert_eq!((branch("main").ahead, branch("main").behind), (1, 0));
    assert_eq!(branch("main").subject, "Main moves on");
    assert!(branch("old").gone);
    // origin/HEAD is not a branch; the deleted remote branch is gone.
    let remotes: Vec<(&str, Vec<&str>)> = found
        .remotes
        .iter()
        .map(|remote| (remote.name.as_str(), remote.branches.iter().map(|b| b.name.as_str()).collect()))
        .collect();
    assert_eq!(remotes, [("origin", vec!["main"])]);
    // The annotated tag counts as the commit it tags.
    assert_eq!(found.tags.iter().find(|tag| tag.name == "v2").unwrap().sha, branch("main").sha);
    assert!(found.tags.iter().any(|tag| tag.name == "v1"));
    assert_eq!(found.stashes.len(), 1);
    assert!(found.stashes[0].message.ends_with("Half done"));
    // new.ts (untracked) and gone.ts (deleted); the stashed edit to a.ts is no longer in the working tree.
    assert_eq!(local_changes(&r.root).unwrap(), Some(2));

    let plain = tempfile::tempdir().unwrap();
    assert!(refs(&plain.path().to_string_lossy()).unwrap().is_none());
    assert!(local_changes(&plain.path().to_string_lossy()).unwrap().is_none());
}

#[test]
fn history_blame_and_churn() {
    let r = feature_branch();
    // edit() is lines 2 to 4: written in the first commit, changed in "Edit a".
    let history = function_history(&r.root, "a.ts", 2, 4);
    assert_eq!(history.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["Edit a", "First"]);
    assert_eq!(history.commits[0].author, "Test");
    assert_eq!(history.authors.iter().map(|a| (a.name.as_str(), a.lines)).collect::<Vec<_>>(), [("Test", 3)]);
    assert_eq!(history.latest.unwrap().subject, "Edit a");
    let busy = churn(&r.root, 30).unwrap();
    assert_eq!(busy["a.ts"].commits, 2);
    assert_eq!(busy["renamed.ts"].commits, 1);
    // A file git does not know has no history, and that is no error.
    assert!(function_history(&r.root, "new.ts", 1, 1).commits.is_empty());
}

#[test]
fn never_runs_the_repositorys_own_diff_driver_textconv_or_filters() {
    let r = feature_branch();
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    let script = outside.path().join("mark.cjs");
    fs::write(&script, format!("require('fs').writeFileSync({:?}, 'ran')\n", marker.to_string_lossy())).unwrap();
    let command = format!("node \"{}\"", script.to_string_lossy().replace('\\', "/"));
    r.sh(&["config", "diff.external", &command]);
    r.sh(&["config", "filter.evil.clean", &command]);
    r.sh(&["config", "filter.evil.smudge", &command]);
    r.sh(&["config", "diff.evil.textconv", &command]);
    r.write(".gitattributes", "*.ts filter=evil diff=evil\n");
    r.write("a.ts", &format!("{A2}// edited\n"));

    // The control: a plain git diff does run them, so the check below means something.
    let _ = Command::new("git").arg("diff").current_dir(&r.root).env("GIT_OPTIONAL_LOCKS", "0").output();
    assert!(marker.exists(), "a plain git diff should have run the configured command");
    fs::remove_file(&marker).unwrap();

    forget_config(&r.root);
    let page = log(&r.root, &LogQuery::default()).unwrap();
    let head = &page.commits[0].sha;
    let _ = commit(&r.root, head).unwrap();
    let _ = file_diff(&r.root, Some(&page.commits[1].sha), head, "renamed.ts", "old.ts", 3).unwrap();
    let _ = refs(&r.root).unwrap();
    let _ = function_history(&r.root, "a.ts", 2, 4);
    let _ = churn(&r.root, 30).unwrap();
    let _ = log(&r.root, &LogQuery { path: Some("a.ts".into()), ..LogQuery::default() }).unwrap();
    let _ = local_changes(&r.root).unwrap();
    // The changes by function's diffs and base.
    let _ = diff::working_tree_diff(&r.root, head).unwrap();
    let _ = diff::commit_diff(&r.root, Some(&page.commits[1].sha), head).unwrap();
    let _ = diff::commit_diff(&r.root, None, head).unwrap();
    let _ = diff::resolve_base(&r.root, diff::Wanted::MergeBase(None), &diff::head(&r.root)).unwrap();
    let _ = file_at(&r.root, head, "a.ts");
    assert!(!marker.exists(), "Haunt ran a command from the repository config");
}

#[test]
fn unified_hunks_merge_nearby_changes_and_keep_context() {
    let before = (1..=20).map(|n| format!("line {n}\n")).collect::<String>();
    let after = before.replace("line 5\n", "line five\n").replace("line 8\n", "line eight\n").replace("line 18\n", "");
    // Whitespace only: no hunk when it is ignored.
    assert!(objects::hunks_with("a\n  b\nc\n", "a\nb  \nc\n", 3, true).is_empty());
    assert_eq!(objects::hunks_with("a\n  b\nc\n", "a\nb  \nc\n", 3, false).len(), 1);
    let found = objects::hunks(&before, &after, 3);
    // 5 and 8 are close enough to share a hunk; 18 has its own.
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].old_start, found[0].old_count, found[0].new_start, found[0].new_count), (2, 10, 2, 10));
    assert_eq!((found[1].old_start, found[1].old_count, found[1].new_count), (15, 6, 5));
    assert!(found[1].lines.iter().any(|line| line.kind == LineKind::Del && line.text == "line 18"));
    assert_eq!(found[1].section, "line 14");
}

#[test]
fn finds_the_last_commit_another_file_mentioned_a_name() {
    let r = Repo::new();
    r.write("lib.ts", "export function format() {}\n");
    r.write("main.ts", "import { format } from './lib'\nformat()\n");
    r.write("other.ts", "export const formatted = 1\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Use format"]);
    r.write("main.ts", "export {}\n");
    r.sh(&["commit", "-q", "-am", "Stop using format"]);
    r.write("other.ts", "export const formatted = 2\n");
    r.sh(&["commit", "-q", "-am", "Touch formatted"]);
    // "formatted" is another word; lib.ts itself does not count.
    assert_eq!(last_mention(&r.root, "format", "lib.ts").unwrap().unwrap().subject, "Stop using format");
    assert!(last_mention(&r.root, "nowhere", "lib.ts").unwrap().is_none());
    assert!(last_mention(&r.root, "bad name", "lib.ts").is_err());
}

#[test]
fn tells_a_cosmetic_change_from_one_in_behaviour() {
    let r = Repo::new();
    r.write("price.ts", "export function price(n: number) {\n  return n * 2\n}\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Add price"]);
    r.write("price.ts", "export function price(n: number) {\n  // Twice the base.\n    return n  *  2\n}\n");
    r.sh(&["commit", "-q", "-am", "Tidy price"]);
    r.write("price.ts", "export function price(n: number) {\n  // Twice the base.\n    return n  *  3\n}\n");
    r.sh(&["commit", "-q", "-am", "Raise price"]);
    let shas: Vec<String> = r.sh(&["log", "--format=%H"]).lines().map(str::to_owned).collect();
    let history = function_history(&r.root, "price.ts", 1, 4);
    let subjects: Vec<&str> = history.commits.iter().map(|commit| commit.subject.as_str()).collect();
    assert_eq!(subjects, ["Raise price", "Tidy price", "Add price"]);
    assert_eq!(history.cosmetic, [shas[1].clone()], "only the tidying is cosmetic; adding it is not");
    assert!(history.complete, "fewer commits than a page: it reaches back to where it began");
}

#[test]
fn lists_the_commits_between_two_tags_with_their_files() {
    let r = Repo::new();
    r.write("a.ts", "export const a = 1\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r.sh(&["tag", "v1"]);
    r.write("a.ts", "export const a = 2\n");
    r.write("b.ts", "export const b = 1\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Change a, add b"]);
    r.sh(&["switch", "-q", "-c", "side"]);
    r.write("c.ts", "export const c = 1\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Add c"]);
    r.sh(&["switch", "-q", "main"]);
    r.sh(&["merge", "-q", "--no-ff", "-m", "Merge side", "side"]);
    r.sh(&["tag", "v2"]);
    let commits = release_commits(&r.root, Some("v1"), "v2").unwrap();
    let seen: Vec<(String, Vec<String>)> =
        commits.iter().map(|item| (item.commit.subject.clone(), item.paths.clone())).collect();
    assert_eq!(
        seen,
        [
            ("Add c".to_owned(), vec!["c.ts".to_owned()]),
            ("Change a, add b".to_owned(), vec!["a.ts".to_owned(), "b.ts".to_owned()])
        ],
        "merges are left out"
    );
    assert_eq!(release_commits(&r.root, None, "v1").unwrap().len(), 1);
    assert!(release_commits(&r.root, Some("--all"), "v2").is_err());
}

#[test]
fn health_lists_the_largest_files_in_history_and_the_ones_for_git_lfs() {
    let r = Repo::new();
    let big = "x".repeat(300 * 1024);
    r.write("small.txt", "a\n");
    r.write("photo.png", &big);
    r.write("data.bin", &"y".repeat(100 * 1024));
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r.write("data.bin", &"z".repeat(100 * 1024));
    r.sh(&["commit", "-q", "-am", "Second version"]);
    r.sh(&["rm", "-q", "data.bin"]);
    r.sh(&["commit", "-q", "-m", "Remove data"]);
    let found = health(&r.root).unwrap();
    assert!(found.bytes > 0);
    let names: Vec<&str> = found.largest.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(&names[..2], ["photo.png", "data.bin"]);
    let data = &found.largest[1];
    assert_eq!(
        (data.versions, data.in_head, data.bytes),
        (2, false, 200 * 1024),
        "both versions, gone from the last commit"
    );
    assert!(found.largest[0].in_head);
    // An image over 256 KiB belongs in Git LFS; once LFS keeps it, it does not.
    assert_eq!(found.lfs.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["photo.png"]);
    assert!(!found.uses_lfs);
    r.write(".gitattributes", "*.png filter=lfs diff=lfs merge=lfs -text\n");
    let found = health(&r.root).unwrap();
    assert!(found.lfs.is_empty() && found.uses_lfs);
}

#[test]
fn a_filter_added_later_through_an_included_config_is_switched_off_too() {
    let r = Repo::new();
    r.write("a.txt", "one\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    // Read once: the list of filters (none) is remembered.
    forget_config(&r.root);
    let _ = local_changes(&r.root).unwrap();

    // A filter now comes in through an included file, which Haunt does not watch.
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    let script = outside.path().join("mark.cjs");
    fs::write(&script, format!("require('fs').writeFileSync({:?}, 'ran')\n", marker.to_string_lossy())).unwrap();
    let command = format!("node \"{}\"", script.to_string_lossy().replace('\\', "/"));
    let included = outside.path().join("extra.gitconfig");
    fs::write(&included, format!("[filter \"late\"]\n\tclean = {command}\n\tsmudge = {command}\n")).unwrap();
    r.sh(&["config", "include.path", &included.to_string_lossy().replace('\\', "/")]);
    r.write(".gitattributes", "*.txt filter=late\n");
    r.write("a.txt", "two\n");

    // Once the remembered list is stale, the new filter is read and switched off.
    std::thread::sleep(std::time::Duration::from_millis(3200));
    let _ = local_changes(&r.root).unwrap();
    let _ = log(&r.root, &LogQuery { path: Some("a.txt".into()), ..LogQuery::default() }).unwrap();
    assert!(!marker.exists(), "the late filter ran during a read");
}

#[test]
fn lists_first_parent_commits_with_their_files_merges_and_renames_included() {
    let r = Repo::new();
    r.write("a.ts", "export const a = 1\n");
    r.write("old.ts", "export const o = 1\nexport const p = 2\nexport const q = 3\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r.sh(&["switch", "-q", "-c", "side"]);
    r.write("side.ts", "export const s = 1\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "Side"]);
    r.sh(&["switch", "-q", "main"]);
    r.sh(&["mv", "old.ts", "new.ts"]);
    r.sh(&["commit", "-q", "-m", "Rename"]);
    r.sh(&["merge", "-q", "--no-ff", "-m", "Merge side", "side"]);
    let found = first_parent_changes(&r.root, 10).unwrap();
    let seen: Vec<(String, bool, Vec<String>)> = found
        .iter()
        .map(|commit| {
            let files = commit.files.iter().map(|(status, old, new)| format!("{status:?} {old} {new}")).collect();
            (commit.info.subject.clone(), commit.parent.is_some(), files)
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("Merge side".to_owned(), true, vec!["Added side.ts side.ts".to_owned()]),
            ("Rename".to_owned(), true, vec!["Renamed old.ts new.ts".to_owned()]),
            ("First".to_owned(), false, vec!["Added a.ts a.ts".to_owned(), "Added old.ts old.ts".to_owned()]),
        ],
        "the side branch's own commit is not on the first-parent line; the merge brings its file"
    );
}

#[test]
fn refs_name_the_branch_of_a_new_repository_and_none_when_detached() {
    let r = Repo::new();
    let fresh = refs(&r.root).unwrap().unwrap();
    assert_eq!((fresh.branch.as_deref(), fresh.sha.as_deref()), (Some("main"), None), "no commit yet");
    r.write("a.txt", "a\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    let head = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
    let on_main = refs(&r.root).unwrap().unwrap();
    assert_eq!((on_main.branch.as_deref(), on_main.sha.as_deref()), (Some("main"), Some(head.as_str())));
    r.sh(&["switch", "-q", "--detach", "HEAD"]);
    let detached = refs(&r.root).unwrap().unwrap();
    assert_eq!((detached.branch.as_deref(), detached.sha.as_deref()), (None, Some(head.as_str())));
    let outside = tempfile::tempdir().unwrap();
    assert!(refs(&outside.path().to_string_lossy()).unwrap().is_none());
}

#[test]
fn a_commit_names_its_branches_and_tags_annotated_or_not() {
    let r = Repo::new();
    r.write("a.txt", "a\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r.sh(&["tag", "light"]);
    r.sh(&["tag", "-a", "heavy", "-m", "A release"]);
    r.write("a.txt", "b\n");
    r.sh(&["commit", "-q", "-am", "Second"]);
    r.sh(&["tag", "-a", "later", "-m", "Not this one"]);
    // Packed refs keep a tag's commit beside it: found that way too.
    r.sh(&["pack-refs", "--all"]);
    let first = r.sh(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let mut names: Vec<String> =
        commit(&r.root, &first).unwrap().refs.iter().map(|d| format!("{:?} {}", d.kind, d.name)).collect();
    names.sort();
    assert_eq!(names, ["Tag heavy", "Tag light"]);
}

#[test]
fn drafts_a_pull_request_from_a_branchs_commits() {
    let r = Repo::new();
    r.write("a.txt", "a\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    let remote = tempfile::tempdir().unwrap();
    let remote_path = remote.path().to_string_lossy().into_owned();
    assert!(Command::new("git").args(["init", "-q", "--bare", &remote_path]).status().unwrap().success());
    r.sh(&["remote", "add", "origin", &remote_path]);
    r.sh(&["push", "-q", "-u", "origin", "main"]);
    r.sh(&["switch", "-q", "-c", "fix/login-redirect"]);
    r.sh(&["commit", "-q", "--allow-empty", "-m", "Keep the page"]);
    r.sh(&["commit", "-q", "--allow-empty", "-m", "Redirect after login"]);

    let draft = crate::git::pull_draft(&r.root, "fix/login-redirect", None).unwrap();
    assert_eq!((draft.remote.as_str(), draft.base.as_str()), ("origin", "main"));
    assert!(!draft.pushed, "not on the remote yet");
    assert_eq!(draft.commits, ["Redirect after login", "Keep the page"]);
    assert_eq!(draft.title, "Login redirect");
    assert_eq!(draft.body, "- Keep the page\n- Redirect after login");

    r.sh(&["push", "-q", "-u", "origin", "fix/login-redirect"]);
    let draft = crate::git::pull_draft(&r.root, "fix/login-redirect", None).unwrap();
    assert!(draft.pushed);
    assert_eq!(draft.bases, ["main"], "its own branch is not a base");
    assert!(crate::git::pull_draft(&r.root, "fix/login-redirect", Some("nope")).is_err());
    assert!(crate::git::pull_draft(&r.root, "--all", None).is_err());
}
