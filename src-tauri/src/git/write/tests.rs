//! Writing, on repositories the tests build: staging whole files and lines, committing and amending, discarding and
//! bringing it back, and the rules: an untrusted folder runs none of its hooks and filters (a trusted one does), a file
//! with a filter is refused there, and a held index.lock makes a write wait and fail, leaving the lock.

use std::fs;

use super::{LineRef, LinesAction, Writer};
use crate::git::local::status;
use crate::git::tests::Repo;

mod network;
mod rebase;
mod undo;

/// A repository with one commit: a.txt (five lines) and b.txt.
fn repo() -> Repo {
    let r = Repo::new();
    r.write("a.txt", "one\ntwo\nthree\nfour\nfive\n");
    r.write("b.txt", "b\n");
    r.sh(&["add", "."]);
    r.sh(&["commit", "-q", "-m", "First"]);
    r
}

fn staged(r: &Repo) -> Vec<String> {
    status(&r.root).unwrap().unwrap().staged.into_iter().map(|file| file.path).collect()
}

fn unstaged(r: &Repo) -> Vec<String> {
    status(&r.root).unwrap().unwrap().unstaged.into_iter().map(|file| file.path).collect()
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| path.to_string()).collect()
}

#[test]
fn stages_and_unstages_whole_files() {
    let r = repo();
    r.write("a.txt", "one\nTWO\nthree\nfour\nfive\n");
    r.write("new.txt", "new\n");
    fs::remove_file(r.path("b.txt")).unwrap();
    let writer = Writer::new(&r.root, true);
    assert_eq!(unstaged(&r), ["a.txt", "b.txt", "new.txt"]);
    writer.stage(&paths(&["a.txt", "b.txt", "new.txt"])).unwrap();
    assert_eq!(staged(&r), ["a.txt", "b.txt", "new.txt"]);
    assert!(unstaged(&r).is_empty());
    writer.unstage(&paths(&["a.txt", "new.txt"])).unwrap();
    assert_eq!(staged(&r), ["b.txt"]);
    assert_eq!(unstaged(&r), ["a.txt", "new.txt"]);
    // Paths are checked: never an option, never outside the folder.
    assert!(writer.stage(&paths(&["--all"])).is_err());
    assert!(writer.stage(&paths(&["../x"])).is_err());
}

#[test]
fn stages_and_unstages_a_few_lines() {
    let r = repo();
    r.write("a.txt", "one\nTWO\nthree\nFOUR\nfive\n");
    let writer = Writer::new(&r.root, true);
    // Stage only TWO (replacing two): FOUR stays unstaged.
    let two = [LineRef { old: Some(2), new: None }, LineRef { old: None, new: Some(2) }];
    writer.lines("a.txt", &two, LinesAction::Stage).unwrap();
    assert_eq!(r.sh(&["show", ":a.txt"]), "one\nTWO\nthree\nfour\nfive\n");
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "one\nTWO\nthree\nFOUR\nfive\n");
    // And take it back out of the index.
    writer
        .lines(
            "a.txt",
            &[LineRef { old: None, new: Some(2) }, LineRef { old: Some(2), new: None }],
            LinesAction::Unstage,
        )
        .unwrap();
    assert_eq!(r.sh(&["show", ":a.txt"]), "one\ntwo\nthree\nfour\nfive\n");
    // A new file's lines cannot be picked.
    r.write("new.txt", "x\n");
    assert!(writer.lines("new.txt", &[LineRef { old: None, new: Some(1) }], LinesAction::Stage).is_err());
}

#[test]
fn commits_amends_and_signs_off() {
    let r = repo();
    r.write("a.txt", "changed\n");
    let writer = Writer::new(&r.root, true);
    writer.stage(&paths(&["a.txt"])).unwrap();
    assert!(writer.commit("  ", false, false).is_err());
    // A line of the message that starts with # is kept: it is not a comment here.
    let sha = writer.commit("Change a\n\n# not a comment", false, false).unwrap();
    assert_eq!(r.sh(&["rev-parse", "HEAD"]).trim(), sha);
    assert_eq!(r.sh(&["log", "-1", "--format=%B"]), "Change a\n\n# not a comment\n\n");
    let box_ = writer.commit_box().unwrap();
    assert!(box_.last_message.unwrap().starts_with("Change a"));
    // Amending, with a sign-off.
    writer.commit("Change a, better", true, true).unwrap();
    assert_eq!(r.sh(&["log", "--format=%s"]), "Change a, better\nFirst\n");
    assert!(r.sh(&["log", "-1", "--format=%B"]).contains("Signed-off-by: Test <test@example.com>"));
}

#[test]
fn discards_and_brings_back_files_lines_and_untracked_files() {
    let r = repo();
    r.write("a.txt", "one\nTWO\nthree\nFOUR\nfive\n");
    r.write("new.txt", "keep me\n");
    let index_before = r.sh(&["ls-files", "-s"]);
    let writer = Writer::new(&r.root, true);

    writer.discard(&paths(&["a.txt", "new.txt"])).unwrap();
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "one\ntwo\nthree\nfour\nfive\n");
    assert!(!r.path("new.txt").exists());
    // The index is untouched by the save.
    assert_eq!(r.sh(&["ls-files", "-s"]), index_before);
    let saved = writer.discarded().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].paths, ["a.txt", "new.txt"]);

    writer.restore(&saved[0].id).unwrap();
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "one\nTWO\nthree\nFOUR\nfive\n");
    assert_eq!(fs::read_to_string(r.path("new.txt")).unwrap(), "keep me\n");
    // Restoring saved what it replaced, and the restored save is gone.
    let after = writer.discarded().unwrap();
    assert_eq!(after.len(), 1);
    assert_ne!(after[0].id, saved[0].id);

    // A few lines: FOUR goes, TWO stays.
    writer
        .lines(
            "a.txt",
            &[LineRef { old: Some(4), new: None }, LineRef { old: None, new: Some(4) }],
            LinesAction::Discard,
        )
        .unwrap();
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "one\nTWO\nthree\nfour\nfive\n");
    let lines_save = writer.discarded().unwrap().remove(0);
    writer.restore(&lines_save.id).unwrap();
    assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "one\nTWO\nthree\nFOUR\nfive\n");
}

#[test]
fn a_discarded_deletion_comes_back_as_a_deletion() {
    let r = repo();
    fs::remove_file(r.path("b.txt")).unwrap();
    let writer = Writer::new(&r.root, true);
    writer.discard(&paths(&["b.txt"])).unwrap();
    assert!(r.path("b.txt").exists());
    let saved = writer.discarded().unwrap().remove(0);
    writer.restore(&saved.id).unwrap();
    assert!(!r.path("b.txt").exists());
}

#[test]
fn unstages_before_the_first_commit() {
    let r = Repo::new();
    r.write("a.txt", "a\n");
    let writer = Writer::new(&r.root, true);
    writer.stage(&paths(&["a.txt"])).unwrap();
    assert_eq!(staged(&r), ["a.txt"]);
    writer.unstage(&paths(&["a.txt"])).unwrap();
    assert!(staged(&r).is_empty());
    assert_eq!(unstaged(&r), ["a.txt"]);
}

/// A hook and a filter that leave a marker file when they run.
fn with_hook_and_filter(r: &Repo) -> (tempfile::TempDir, std::path::PathBuf) {
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    let script = outside.path().join("mark.cjs");
    fs::write(
        &script,
        format!(
            "require('fs').writeFileSync({:?}, 'ran'); process.stdin.pipe(process.stdout)\n",
            marker.to_string_lossy()
        ),
    )
    .unwrap();
    let command = format!("node \"{}\"", script.to_string_lossy().replace('\\', "/"));
    let hook = r.path(".git/hooks/pre-commit");
    fs::write(&hook, format!("#!/bin/sh\n{command} < /dev/null > /dev/null\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    r.sh(&["config", "filter.mark.clean", &command]);
    r.write(".gitattributes", "*.lfs filter=mark\n");
    r.sh(&["add", ".gitattributes"]);
    r.sh(&["commit", "-q", "--no-verify", "-m", "Attributes"]);
    (outside, marker)
}

#[test]
fn an_untrusted_folder_runs_none_of_its_hooks_and_refuses_filtered_files() {
    let r = repo();
    let (_outside, marker) = with_hook_and_filter(&r);
    assert!(!marker.exists());
    let writer = Writer::new(&r.root, false);
    r.write("data.lfs", "big\n");
    let refused = writer.stage(&paths(&["data.lfs"])).unwrap_err();
    assert!(refused.contains("trusted folder"), "{refused}");
    r.write("a.txt", "changed\n");
    writer.stage(&paths(&["a.txt"])).unwrap();
    writer.commit("Untrusted commit", false, false).unwrap();
    writer.discard(&paths(&["b.txt"])).ok();
    assert!(!marker.exists(), "an untrusted folder ran its own hook or filter");
    let box_ = writer.commit_box().unwrap();
    assert_eq!(box_.hooks, ["pre-commit"]);
    assert!(!box_.trusted);
}

#[test]
fn a_trusted_folder_runs_its_hooks_and_filters() {
    let r = repo();
    let (_outside, marker) = with_hook_and_filter(&r);
    let writer = Writer::new(&r.root, true);
    r.write("data.lfs", "big\n");
    writer.stage(&paths(&["data.lfs"])).unwrap();
    assert!(marker.exists(), "the filter should run in a trusted folder");
    fs::remove_file(&marker).unwrap();
    writer.commit("Trusted commit", false, false).unwrap();
    assert!(marker.exists(), "the hook should run in a trusted folder");
}

#[test]
fn a_held_index_lock_makes_a_write_wait_and_fail_and_stays() {
    let r = repo();
    r.write("a.txt", "changed\n");
    let lock = r.path(".git/index.lock");
    fs::write(&lock, "").unwrap();
    let writer = Writer::new(&r.root, true);
    let started = std::time::Instant::now();
    let error = writer.stage(&paths(&["a.txt"])).unwrap_err();
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    assert!(error.contains("index.lock"), "{error}");
    assert!(lock.exists(), "Haunt must leave the lock alone");
    fs::remove_file(&lock).unwrap();
    writer.stage(&paths(&["a.txt"])).unwrap();
    // Every command is in the activity list, the failed ones too.
    let activity = super::activity(&r.root);
    assert!(activity.iter().any(|entry| entry.command.starts_with("git add -A -- a.txt") && entry.ok));
}

/* ---------- branches, stashes and tags ---------- */

mod refs {
    use std::fs;

    use super::super::{SwitchMode, Writer};
    use super::repo;

    fn current(r: &crate::git::tests::Repo) -> String {
        r.sh(&["branch", "--show-current"]).trim().to_owned()
    }

    #[test]
    fn creates_switches_renames_and_deletes_branches() {
        let r = repo();
        let first = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        r.write("c.txt", "c\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Second"]);
        let writer = Writer::new(&r.root, true);

        // A branch at the first commit, without switching; then one switched to at once.
        writer.create_branch("old", Some(&first), false).unwrap();
        assert_eq!(r.sh(&["rev-parse", "old"]).trim(), first);
        assert_eq!(current(&r), "main");
        writer.create_branch("feature/x", None, true).unwrap();
        assert_eq!(current(&r), "feature/x");
        writer.switch_branch("main", SwitchMode::Carry).unwrap();
        writer.rename_branch("feature/x", "feature/y").unwrap();
        assert!(r.sh(&["branch", "--list", "feature/y"]).contains("feature/y"));

        // Names are checked; a revision is never an option.
        assert!(writer.create_branch("bad name", None, false).is_err());
        assert!(writer.create_branch("-x", None, false).is_err());
        assert!(writer.create_branch("ok", Some("--orphan"), false).is_err());

        // A branch whose commits another has goes at once; one with its own commits only when confirmed.
        writer.delete_branch("old", false).unwrap();
        writer.switch_branch("feature/y", SwitchMode::Carry).unwrap();
        r.write("d.txt", "d\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Only here"]);
        writer.switch_branch("main", SwitchMode::Carry).unwrap();
        assert_eq!(writer.unmerged("feature/y").unwrap(), 1);
        assert!(writer.delete_branch("feature/y", false).unwrap_err().contains("confirm"));
        writer.delete_branch("feature/y", true).unwrap();
        assert!(r.sh(&["branch", "--list", "feature/y"]).trim().is_empty());
    }

    #[test]
    fn switching_carries_or_stashes_local_changes() {
        let r = repo();
        r.sh(&["branch", "other"]);
        r.write("a.txt", "changed\n");
        r.write("new.txt", "untracked\n");
        let writer = Writer::new(&r.root, true);
        writer.switch_branch("other", SwitchMode::Carry).unwrap();
        assert_eq!(current(&r), "other");
        assert_eq!(fs::read_to_string(r.path("a.txt")).unwrap(), "changed\n");

        writer.switch_branch("main", SwitchMode::Stash).unwrap();
        assert_eq!(current(&r), "main");
        assert!(r.sh(&["status", "--porcelain"]).trim().is_empty(), "the changes went into a stash");
        assert!(r.sh(&["stash", "list"]).contains("Before switching to main"));
        assert!(!r.path("new.txt").exists());
    }

    #[test]
    fn switching_to_a_remote_branch_makes_a_local_one_that_tracks_it() {
        let r = repo();
        r.sh(&["remote", "add", "origin", &r.root]);
        r.sh(&["branch", "feature"]);
        r.sh(&["update-ref", "refs/remotes/origin/feature", "feature"]);
        r.sh(&["branch", "-D", "feature"]);
        let writer = Writer::new(&r.root, true);
        writer.switch_branch("origin/feature", SwitchMode::Carry).unwrap();
        assert_eq!(current(&r), "feature");
        assert_eq!(r.sh(&["rev-parse", "--abbrev-ref", "feature@{upstream}"]).trim(), "origin/feature");
        // Again: the local branch is there now, and is switched to.
        writer.switch_branch("main", SwitchMode::Carry).unwrap();
        writer.switch_branch("origin/feature", SwitchMode::Carry).unwrap();
        assert_eq!(current(&r), "feature");
    }

    #[test]
    fn stashes_applies_pops_drops_and_brings_back() {
        let r = repo();
        let writer = Writer::new(&r.root, true);
        assert!(writer.stash(None, false).is_err(), "nothing to stash");
        r.write("a.txt", "one\n");
        r.write("new.txt", "untracked\n");
        writer.stash(Some("First"), true).unwrap();
        assert!(!r.path("new.txt").exists());
        writer.apply_stash(0, false).unwrap();
        assert_eq!(fs::read_to_string(r.path("new.txt")).unwrap(), "untracked\n");
        assert_eq!(r.sh(&["stash", "list"]).lines().count(), 1, "apply keeps the stash");
        r.sh(&["checkout", "--", "a.txt"]);
        fs::remove_file(r.path("new.txt")).unwrap();
        writer.apply_stash(0, true).unwrap();
        assert!(r.sh(&["stash", "list"]).trim().is_empty(), "pop drops it");

        writer.stash(Some("Second"), true).unwrap();
        writer.drop_stash(0).unwrap();
        assert!(r.sh(&["stash", "list"]).trim().is_empty());
        let dropped = writer.dropped().unwrap();
        assert_eq!(dropped.len(), 1);
        assert!(dropped[0].message.contains("Second"));
        writer.restore_stash(&dropped[0].id).unwrap();
        assert!(r.sh(&["stash", "list"]).contains("On main: Second"), "the message comes back whole");
        assert!(writer.dropped().unwrap().is_empty());
    }

    #[test]
    fn creates_and_deletes_tags() {
        let r = repo();
        let writer = Writer::new(&r.root, true);
        writer.create_tag("v1", None, None).unwrap();
        writer.create_tag("v2", Some("HEAD"), Some("The second\n\n# kept")).unwrap();
        assert_eq!(r.sh(&["cat-file", "-t", "v1"]).trim(), "commit");
        assert_eq!(r.sh(&["cat-file", "-t", "v2"]).trim(), "tag");
        assert!(r.sh(&["tag", "-l", "-n9", "v2"]).contains("The second"));
        assert!(writer.create_tag("bad..name", None, None).is_err());
        writer.delete_tag("v1").unwrap();
        assert!(r.sh(&["tag", "-l", "v1"]).trim().is_empty());
    }

    #[test]
    fn an_untrusted_folder_with_filters_does_not_switch_or_stash() {
        let r = repo();
        r.sh(&["branch", "other"]);
        r.sh(&["config", "filter.lfs.clean", "git-lfs clean -- %f"]);
        // A filter driver alone (as in Git for Windows' own config) is not enough: the files must use it.
        let writer = Writer::new(&r.root, false);
        writer.switch_branch("other", SwitchMode::Carry).unwrap();
        writer.switch_branch("main", SwitchMode::Carry).unwrap();
        r.write(".gitattributes", "*.bin filter=lfs diff=lfs merge=lfs -text\n");
        r.sh(&["add", ".gitattributes"]);
        r.sh(&["commit", "-m", "lfs"]);
        assert!(writer.switch_branch("other", SwitchMode::Carry).unwrap_err().contains("trusted"));
        r.write("a.txt", "changed\n");
        assert!(writer.stash(None, false).unwrap_err().contains("trusted"));
        // Creating a branch without switching writes no files: allowed.
        writer.create_branch("third", None, false).unwrap();
        assert_eq!(current(&r), "main");
    }
}

mod worktrees {
    use super::super::{worktrees, Writer};
    use super::repo;

    #[test]
    fn adds_lists_removes_and_prunes_worktrees() {
        let r = repo();
        r.sh(&["branch", "feature"]);
        let place = tempfile::tempdir().unwrap();
        let there = place.path().join("repo-feature").to_string_lossy().into_owned();
        let writer = Writer::new(&r.root, false);

        let folder = writer.add_worktree(&there, "feature", false, None).unwrap();
        assert!(std::path::Path::new(&folder).join("a.txt").exists(), "the branch is checked out there");
        let fresh = place.path().join("repo-new").to_string_lossy().into_owned();
        writer.add_worktree(&fresh, "topic", true, Some("HEAD")).unwrap();
        assert!(writer.add_worktree(&there, "main", false, None).is_err(), "a folder that is not empty");
        assert!(writer.add_worktree("relative/path", "feature", false, None).is_err());

        let listed = worktrees(&r.root).unwrap();
        let seen: Vec<(Option<&str>, bool)> =
            listed.iter().map(|worktree| (worktree.branch.as_deref(), worktree.main)).collect();
        assert_eq!(seen, [(Some("main"), true), (Some("feature"), false), (Some("topic"), false)]);
        assert!(writer.remove_worktree(&r.root, false).is_err(), "not the repository's own folder");

        // A change not committed: refused, then removed with force.
        std::fs::write(std::path::Path::new(&folder).join("a.txt"), "changed\n").unwrap();
        let refused = writer.remove_worktree(&folder, false).unwrap_err();
        assert!(refused.contains("not committed"), "{refused}");
        writer.remove_worktree(&folder, true).unwrap();
        assert!(!std::path::Path::new(&folder).exists());
        assert!(r.sh(&["branch", "--list", "feature"]).contains("feature"), "the branch stays");

        // A worktree whose folder was deleted by hand is prunable, then forgotten.
        std::fs::remove_dir_all(&fresh).unwrap();
        assert!(worktrees(&r.root).unwrap().iter().any(|worktree| worktree.prunable));
        writer.prune_worktrees().unwrap();
        assert_eq!(worktrees(&r.root).unwrap().len(), 1);
    }
}

mod bisect {
    use super::super::{bisect_state, BisectMark, Writer};
    use crate::git::tests::Repo;

    #[test]
    fn finds_the_first_bad_commit_step_by_step_and_tests_another_on_request() {
        let r = Repo::new();
        for n in 1..=8 {
            r.write("n.txt", &format!("{n}\n"));
            r.sh(&["add", "."]);
            r.sh(&["commit", "-q", "-m", &format!("Commit {n}")]);
        }
        let shas: Vec<String> = r.sh(&["log", "--format=%H"]).lines().map(str::to_owned).collect();
        // Commit 6 is the first bad one: n.txt holds 6 or more from there.
        let bad_from = 6;
        let writer = Writer::new(&r.root, false);
        assert!(!bisect_state(&r.root).active);
        writer.bisect_start("HEAD", &shas[7]).unwrap();
        let state = bisect_state(&r.root);
        assert!(state.active);
        assert_eq!(state.left, 7, "commits 2 to 8 are in play");
        // Test one picked by hand, then follow git's picks.
        writer.bisect_jump(&shas[1]).unwrap();
        assert!(writer.bisect_jump(&shas[7]).is_err(), "the good one is not in play");
        let mut guard = 0;
        loop {
            let state = bisect_state(&r.root);
            if let Some(culprit) = state.culprit {
                assert_eq!(culprit.subject, format!("Commit {bad_from}"));
                break;
            }
            let n: u32 = std::fs::read_to_string(r.path("n.txt")).unwrap().trim().parse().unwrap();
            writer.bisect_mark(if n >= bad_from { BisectMark::Bad } else { BisectMark::Good }).unwrap();
            guard += 1;
            assert!(guard < 10, "it ends");
        }
        writer.bisect_stop().unwrap();
        assert!(!bisect_state(&r.root).active);
        assert_eq!(r.sh(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main", "back on the branch it started from");
    }
}

mod patches {
    use super::super::patches::read_patch;
    use super::super::Writer;
    use crate::git::tests::Repo;

    #[test]
    fn saves_previews_and_applies_patches() {
        let from = Repo::new();
        from.write("a.txt", "one\n");
        from.sh(&["add", "."]);
        from.sh(&["commit", "-q", "-m", "First"]);
        let base = from.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        from.write("a.txt", "one\ntwo\n");
        from.sh(&["commit", "-q", "-am", "Add two"]);
        from.write("b.txt", "b\n");
        from.sh(&["add", "."]);
        from.sh(&["commit", "-q", "-m", "Add b"]);
        let out = tempfile::tempdir().unwrap();
        let folder = crate::paths::plain(std::fs::canonicalize(out.path()).unwrap()).to_string_lossy().into_owned();
        let writer = Writer::new(&from.root, false);
        let one = writer.save_patches(None, "HEAD", &folder).unwrap();
        assert_eq!(one.len(), 1, "one commit");
        let files = writer.save_patches(Some(&base), "HEAD", &folder).unwrap();
        assert_eq!(files.len(), 2);
        let info = read_patch(&files[0]).unwrap();
        assert_eq!((info.subject.as_str(), info.paths.clone()), ("Add two", vec!["a.txt".to_owned()]));
        assert!(info.author.contains("Test"));
        assert!(writer.save_patches(None, "HEAD", "relative").is_err());

        // Another clone at the first commit: the preview leaves everything alone, then both apply as commits.
        let to = Repo::new();
        to.write("a.txt", "one\n");
        to.sh(&["add", "."]);
        to.sh(&["commit", "-q", "-m", "First"]);
        let writer = Writer::new(&to.root, false);
        let preview = writer.preview_patches(&files).unwrap();
        let commit = preview.commit.expect("they apply");
        assert_eq!(to.sh(&["show", "--format=", "--name-only", &commit]).trim(), "a.txt\nb.txt");
        assert_eq!(to.sh(&["status", "--porcelain"]), "", "files and index untouched");
        assert_eq!(to.sh(&["rev-list", "--count", "HEAD"]).trim(), "1");
        writer.apply_patches(&files, true).unwrap();
        assert_eq!(to.sh(&["log", "--format=%s"]).trim(), "Add b\nAdd two\nFirst");

        // A patch that does not apply (a.txt is another text there): the preview says so, and am leaves nothing behind.
        let clash = Repo::new();
        clash.write("a.txt", "uno\n");
        clash.sh(&["add", "."]);
        clash.sh(&["commit", "-q", "-m", "Other first"]);
        let writer = Writer::new(&clash.root, false);
        let preview = writer.preview_patches(&files[..1]).unwrap();
        assert!(preview.commit.is_none() && preview.problem.is_some());
        assert!(writer.apply_patches(&files[..1], true).is_err());
        assert_eq!(clash.sh(&["log", "--format=%s"]).trim(), "Other first");
        assert_eq!(clash.sh(&["status", "--porcelain"]), "");
        assert!(!clash.path(".git/rebase-apply").exists(), "the failed am was undone");

        // In the working tree only.
        let other = Repo::new();
        other.write("a.txt", "one\n");
        other.sh(&["add", "."]);
        other.sh(&["commit", "-q", "-m", "First"]);
        Writer::new(&other.root, false).apply_patches(&files[..1], false).unwrap();
        assert_eq!(other.sh(&["status", "--porcelain"]).trim(), "M a.txt");
    }
}
