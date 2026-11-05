//! Undoing, merging and conflicts: revert and cherry-pick, a reset of each kind (a hard one saving your changes first),
//! undoing the last commit, a clean merge and one that stops on conflicts, resolving them by side or by text, continue
//! and abort, and a pull that leaves its conflicts for the conflict view.

use std::fs;

use super::repo;
use crate::git::tests::Repo;
use crate::git::write::{Outcome, PullMode, PullResult, ResetMode, Side, Writer};

fn commit(r: &Repo, path: &str, text: &str, message: &str) -> String {
    r.write(path, text);
    r.sh(&["add", path]);
    r.sh(&["commit", "-q", "-m", message]);
    r.sh(&["rev-parse", "HEAD"]).trim().to_owned()
}

fn head(r: &Repo) -> String {
    r.sh(&["rev-parse", "HEAD"]).trim().to_owned()
}

fn read(r: &Repo, path: &str) -> String {
    fs::read_to_string(r.path(path)).unwrap()
}

/// A repository where main and `other` both changed b.txt: merging them conflicts.
fn apart() -> Repo {
    let r = repo();
    r.sh(&["branch", "other"]);
    commit(&r, "b.txt", "main's b\n", "Main's b");
    r.sh(&["checkout", "-q", "other"]);
    commit(&r, "b.txt", "other's b\n", "Other's b");
    commit(&r, "c.txt", "c\n", "Other's c");
    r.sh(&["checkout", "-q", "main"]);
    r
}

#[test]
fn reverts_and_cherry_picks() {
    let r = repo();
    let change = commit(&r, "b.txt", "changed\n", "Change b");
    let writer = Writer::new(&r.root, true);
    assert_eq!(writer.revert(&change).unwrap(), Outcome::Done);
    assert_eq!(read(&r, "b.txt"), "b\n");
    assert!(r.sh(&["log", "-1", "--format=%s"]).starts_with("Revert \"Change b\""));

    r.sh(&["checkout", "-q", "-b", "other", "HEAD~2"]);
    assert_eq!(writer.cherry_pick(&change).unwrap(), Outcome::Done);
    assert_eq!(read(&r, "b.txt"), "changed\n");
    // The branch has it already: nothing to do, and nothing left half done.
    assert_eq!(writer.cherry_pick(&change).unwrap(), Outcome::UpToDate);
    assert!(writer.operation().unwrap().is_none());
    assert!(writer.revert("--all").is_err(), "a name from the window is never an option");
}

#[test]
fn resets_soft_mixed_and_hard_saving_local_changes_first() {
    let r = repo();
    let first = head(&r);
    commit(&r, "b.txt", "second\n", "Second");
    let writer = Writer::new(&r.root, true);

    writer.reset(&first, ResetMode::Soft).unwrap();
    assert_eq!(head(&r), first);
    assert_eq!(r.sh(&["diff", "--cached", "--name-only"]).trim(), "b.txt", "a soft reset keeps the changes staged");

    r.sh(&["commit", "-q", "-m", "Again"]);
    writer.reset(&first, ResetMode::Mixed).unwrap();
    assert!(r.sh(&["diff", "--cached", "--name-only"]).trim().is_empty());
    assert_eq!(read(&r, "b.txt"), "second\n", "a mixed reset keeps the changes in the files");

    // Hard: the files go back, and what was there is saved as a discard first.
    r.write("a.txt", "local edit\n");
    writer.reset(&first, ResetMode::Hard).unwrap();
    assert_eq!(read(&r, "b.txt"), "b\n");
    assert_eq!(read(&r, "a.txt"), "one\ntwo\nthree\nfour\nfive\n");
    let saved = writer.discarded().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].paths, ["a.txt", "b.txt"]);
    writer.restore(&saved[0].id).unwrap();
    assert_eq!(read(&r, "a.txt"), "local edit\n");
}

#[test]
fn undoes_the_last_commit_keeping_its_changes_staged() {
    let r = repo();
    let first = head(&r);
    commit(&r, "b.txt", "second\n", "Second");
    let writer = Writer::new(&r.root, true);
    writer.undo_commit().unwrap();
    assert_eq!(head(&r), first);
    assert_eq!(r.sh(&["diff", "--cached", "--name-only"]).trim(), "b.txt");
    // The first commit has nothing before it.
    writer.reset(&first, ResetMode::Hard).unwrap();
    assert!(writer.undo_commit().unwrap_err().contains("first commit"));
}

#[test]
fn merges_cleanly_or_stops_on_conflicts_resolved_by_side_or_by_text() {
    let r = repo();
    r.sh(&["checkout", "-q", "-b", "other"]);
    commit(&r, "c.txt", "c\n", "C");
    r.sh(&["checkout", "-q", "main"]);
    let writer = Writer::new(&r.root, true);
    assert_eq!(writer.merge("other").unwrap(), Outcome::Done);
    assert_eq!(writer.merge("other").unwrap(), Outcome::UpToDate);

    let r = apart();
    let writer = Writer::new(&r.root, true);
    let before = head(&r);
    assert_eq!(writer.merge("other").unwrap(), Outcome::Conflicts);
    let going = writer.operation().unwrap().unwrap();
    assert_eq!(crate::said::english(&going.title), "Merging other into main");
    assert_eq!((going.ours.as_str(), going.theirs.as_str()), ("main", "other"));
    assert_eq!(going.conflicted, ["b.txt"]);
    // Another merge waits until this one is done.
    assert!(writer.merge("other").unwrap_err().contains("not finished"));

    let file = writer.conflict("b.txt").unwrap();
    assert_eq!(file.base.as_deref(), Some("b\n"));
    assert_eq!(file.ours.as_deref(), Some("main's b\n"));
    assert_eq!(file.theirs.as_deref(), Some("other's b\n"));
    assert!(file.result.unwrap().contains("<<<<<<<"));
    assert!(writer.conflict("a.txt").is_err(), "a file without conflicts has no conflict view");

    // Continuing waits for the last conflict.
    assert!(writer.proceed().unwrap_err().contains("1 files still conflict"));
    writer.resolve("b.txt", Some("both\n")).unwrap();
    assert!(writer.operation().unwrap().unwrap().conflicted.is_empty());
    assert_eq!(writer.proceed().unwrap(), Outcome::Done);
    assert!(writer.operation().unwrap().is_none());
    assert_eq!(r.sh(&["rev-list", "--count", "--merges", "HEAD"]).trim(), "1");
    assert_eq!(read(&r, "b.txt"), "both\n");
    assert_ne!(head(&r), before);

    // Taking a side, then aborting: everything as it was.
    let r = apart();
    let writer = Writer::new(&r.root, true);
    let before = head(&r);
    writer.merge("other").unwrap();
    writer.take("b.txt", Side::Theirs).unwrap();
    assert_eq!(read(&r, "b.txt"), "other's b\n");
    writer.abort().unwrap();
    assert!(writer.operation().unwrap().is_none());
    assert_eq!(head(&r), before);
    assert_eq!(read(&r, "b.txt"), "main's b\n");
    assert!(r.sh(&["status", "--porcelain"]).trim().is_empty());
}

#[test]
fn a_revert_or_cherry_pick_that_conflicts_is_continued_or_aborted() {
    let r = apart();
    let writer = Writer::new(&r.root, true);
    let theirs = r.sh(&["rev-parse", "other~1"]).trim().to_owned();
    assert_eq!(writer.cherry_pick(&theirs).unwrap(), Outcome::Conflicts);
    let going = writer.operation().unwrap().unwrap();
    assert!(crate::said::english(&going.title).starts_with("Cherry-picking \"Other's b\""), "{}", going.title);
    writer.take("b.txt", Side::Theirs).unwrap();
    assert_eq!(writer.proceed().unwrap(), Outcome::Done);
    assert!(writer.operation().unwrap().is_none());
    assert_eq!(read(&r, "b.txt"), "other's b\n");
    assert_eq!(r.sh(&["log", "-1", "--format=%s"]).trim(), "Other's b");

    // Reverting main's own change of b.txt after that conflicts too; aborting puts it back.
    let mine = r.sh(&["rev-parse", "HEAD~1"]).trim().to_owned();
    let before = head(&r);
    assert_eq!(writer.revert(&mine).unwrap(), Outcome::Conflicts);
    assert!(crate::said::english(&writer.operation().unwrap().unwrap().title).starts_with("Reverting \"Main's b\""));
    writer.abort().unwrap();
    assert_eq!(head(&r), before);
    assert!(r.sh(&["status", "--porcelain"]).trim().is_empty());
}

#[test]
fn a_pull_that_conflicts_leaves_the_conflicts_for_the_conflict_view() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().to_string_lossy().replace('\\', "/");
    assert!(std::process::Command::new("git")
        .args(["init", "-q", "--bare", "-b", "main", &remote])
        .status()
        .unwrap()
        .success());
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    Writer::new(&a.root, true).push(None, false, &|_| {}).unwrap();
    let b = Repo::new();
    b.sh(&["remote", "add", "origin", &remote]);
    b.sh(&["fetch", "-q", "origin"]);
    b.sh(&["reset", "-q", "--hard", "origin/main"]);
    b.sh(&["branch", "-q", "-u", "origin/main"]);
    commit(&a, "b.txt", "a's b\n", "A's b");
    Writer::new(&a.root, true).push(None, false, &|_| {}).unwrap();
    commit(&b, "b.txt", "b's b\n", "B's b");

    let puller = Writer::new(&b.root, true);
    assert_eq!(puller.pull(PullMode::Merge, &|_| {}).unwrap(), PullResult::Conflicts);
    assert_eq!(puller.operation().unwrap().unwrap().conflicted, ["b.txt"]);
    puller.abort().unwrap();
    assert!(b.sh(&["status", "--porcelain"]).trim().is_empty());
}

#[test]
fn an_untrusted_folder_with_filters_does_not_merge_reset_hard_or_revert() {
    let r = apart();
    r.sh(&["config", "filter.lfs.clean", "git-lfs clean -- %f"]);
    r.write(".gitattributes", "*.bin filter=lfs\n");
    r.sh(&["add", ".gitattributes"]);
    r.sh(&["commit", "-q", "-m", "LFS"]);
    let writer = Writer::new(&r.root, false);
    assert!(writer.merge("other").unwrap_err().contains("trusted"));
    assert!(writer.reset("HEAD~1", ResetMode::Hard).unwrap_err().contains("trusted"));
    assert!(writer.revert("HEAD").unwrap_err().contains("trusted"));
    // A soft reset writes no files: allowed.
    writer.reset("HEAD~1", ResetMode::Soft).unwrap();
}

/// The window's Undo (git/write/last.rs): a write noted as the API notes it, then undone while nothing moved since.
#[test]
fn undoes_the_last_write_while_nothing_moved_since() {
    use crate::git::write::{forget_last, SwitchMode, Undo};
    let r = repo();
    let writer = Writer::new(&r.root, true);
    let first = head(&r);
    let noted = |title: &str, undo: Undo, write: &dyn Fn() -> Result<(), String>| {
        let before = writer.snapshot();
        forget_last(&r.root);
        write().unwrap();
        writer.record(title.to_owned(), before, undo)
    };

    // A commit goes back to staged.
    r.write("a.txt", "changed\n");
    r.sh(&["add", "a.txt"]);
    let last = noted("Committed", Undo::Reset("--soft"), &|| writer.commit("Second", false, false).map(drop)).unwrap();
    assert!(last.undo.contains("stay staged"), "{}", last.undo);
    assert_eq!(writer.last_action().unwrap().title, "Committed");
    assert_eq!(writer.undo_last().unwrap(), "Committed");
    assert_eq!(head(&r), first);
    assert_eq!(r.sh(&["diff", "--cached", "--name-only"]).trim(), "a.txt");
    assert!(writer.last_action().is_none() && writer.undo_last().is_err());

    // A deleted branch comes back where it was; a switch goes back.
    r.sh(&["commit", "-q", "-m", "Second"]);
    r.sh(&["branch", "side", &first]);
    let undo = Undo::Recreate { name: "side".into(), sha: first.clone() };
    noted("Deleted side", undo, &|| writer.delete_branch("side", true)).unwrap();
    writer.undo_last().unwrap();
    assert_eq!(r.sh(&["rev-parse", "side"]).trim(), first);
    noted("Switched to side", Undo::SwitchBack, &|| writer.switch_branch("side", SwitchMode::Carry)).unwrap();
    writer.undo_last().unwrap();
    assert_eq!(r.sh(&["branch", "--show-current"]).trim(), "main");

    // A new branch switched to: back, and gone.
    noted("Created new", Undo::RemoveCreated { name: "new".into() }, &|| writer.create_branch("new", None, true))
        .unwrap();
    assert!(writer.last_action().unwrap().undo.contains("Back to main"));
    writer.undo_last().unwrap();
    assert_eq!(r.sh(&["branch", "--show-current"]).trim(), "main");
    assert!(r.sh(&["branch", "--list", "new"]).trim().is_empty());

    // A hard reset moves files back; a commit made since leaves nothing to undo.
    let second = head(&r);
    noted("Reset", Undo::Reset("--keep"), &|| writer.reset(&first, ResetMode::Hard)).unwrap();
    writer.undo_last().unwrap();
    assert_eq!(head(&r), second);
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "changed\n");
    noted("Reset", Undo::Reset("--keep"), &|| writer.reset(&first, ResetMode::Hard)).unwrap();
    commit(&r, "c.txt", "c\n", "Third");
    assert!(writer.last_action().is_none());
    assert!(writer.undo_last().unwrap_err().contains("changed since"));

    // Nothing moved (a reset to where it is already): nothing to undo.
    assert!(noted("Reset", Undo::Reset("--keep"), &|| writer.reset("HEAD", ResetMode::Soft)).is_none());
}
