//! Fetch, pull and push against bare repositories on disk: a normal push, a rejected one, and a lease that fails when
//! the remote moved; pull's fast-forward, merge and rebase, and a conflict undone; and an untrusted fetch that runs
//! nothing of the repository's own.

use std::fs;

use super::repo;
use crate::git::tests::Repo;
use crate::git::write::{PullMode, PullResult, PushResult, Writer};

fn quiet(_: &str) {}

/// A bare repository to push to, and its path as git takes it.
fn bare() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().replace('\\', "/");
    let out = std::process::Command::new("git").args(["init", "-q", "--bare", "-b", "main", &path]).output().unwrap();
    assert!(out.status.success());
    (dir, path)
}

fn remote_sha(remote: &str, reference: &str) -> String {
    let out = std::process::Command::new("git").args(["--git-dir", remote, "rev-parse", reference]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A second clone of `remote`, on main, tracking origin/main.
fn clone(remote: &str) -> Repo {
    let r = Repo::new();
    r.sh(&["remote", "add", "origin", remote]);
    r.sh(&["fetch", "-q", "origin"]);
    r.sh(&["reset", "-q", "--hard", "origin/main"]);
    r.sh(&["branch", "-q", "-u", "origin/main"]);
    r
}

fn commit(r: &Repo, path: &str, text: &str, message: &str) {
    r.write(path, text);
    r.sh(&["add", path]);
    r.sh(&["commit", "-q", "-m", message]);
}

fn head(r: &Repo) -> String {
    r.sh(&["rev-parse", "HEAD"]).trim().to_owned()
}

#[test]
fn pushes_sets_the_upstream_and_overwrites_only_with_a_lease() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    let pusher = Writer::new(&a.root, true);
    assert_eq!(pusher.push(None, false, &quiet).unwrap(), PushResult::Pushed);
    assert_eq!(a.sh(&["config", "branch.main.remote"]).trim(), "origin", "the first push sets the upstream");
    assert_eq!(remote_sha(&remote, "main"), head(&a));

    let b = clone(&remote);
    commit(&b, "b.txt", "from b\n", "B");
    commit(&a, "a.txt", "from a\n", "A");
    assert_eq!(pusher.push(None, false, &quiet).unwrap(), PushResult::Pushed);

    let other = Writer::new(&b.root, true);
    assert_eq!(other.push(None, false, &quiet).unwrap(), PushResult::Rejected);
    // b last saw main before A: the lease fails and A stays.
    let stale = other.push(None, true, &quiet).unwrap_err();
    assert!(stale.contains("has changed"), "{stale}");
    assert_eq!(remote_sha(&remote, "main"), head(&a));
    // Once b has fetched (and seen A), overwriting is its choice.
    other.fetch(&quiet).unwrap();
    assert_eq!(other.push(None, true, &quiet).unwrap(), PushResult::Pushed);
    assert_eq!(remote_sha(&remote, "main"), head(&b));
}

#[test]
fn asks_which_remote_when_there_are_several() {
    let (_one, first) = bare();
    let (_two, second) = bare();
    let r = repo();
    r.sh(&["remote", "add", "one", &first]);
    r.sh(&["remote", "add", "two", &second]);
    let writer = Writer::new(&r.root, true);
    assert_eq!(
        writer.push(None, false, &quiet).unwrap(),
        PushResult::ChooseRemote { remotes: vec!["one".into(), "two".into()] }
    );
    assert!(writer.push(Some("three"), false, &quiet).is_err());
    assert_eq!(writer.push(Some("two"), false, &quiet).unwrap(), PushResult::Pushed);
    assert_eq!(remote_sha(&second, "main"), head(&r));
    assert_eq!(r.sh(&["config", "branch.main.remote"]).trim(), "two");
}

#[test]
fn pulls_fast_forward_then_rebases_or_merges_and_undoes_a_conflict() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    let pusher = Writer::new(&a.root, true);
    pusher.push(None, false, &quiet).unwrap();
    let b = clone(&remote);
    let puller = Writer::new(&b.root, true);

    commit(&a, "c.txt", "c\n", "C");
    pusher.push(None, false, &quiet).unwrap();
    assert_eq!(puller.pull(PullMode::FastForward, &quiet).unwrap(), PullResult::Pulled);
    assert_eq!(head(&b), head(&a));
    assert_eq!(puller.pull(PullMode::FastForward, &quiet).unwrap(), PullResult::UpToDate);

    // Apart: both sides have a commit.
    commit(&a, "d.txt", "d\n", "D");
    pusher.push(None, false, &quiet).unwrap();
    commit(&b, "e.txt", "e\n", "E");
    assert_eq!(puller.pull(PullMode::FastForward, &quiet).unwrap(), PullResult::Diverged);
    assert_eq!(puller.pull(PullMode::Rebase, &quiet).unwrap(), PullResult::Pulled);
    assert_eq!(b.sh(&["rev-list", "--count", "--merges", "HEAD"]).trim(), "0", "a rebase leaves no merge commit");
    assert!(b.path("d.txt").exists() && b.path("e.txt").exists());

    commit(&a, "f.txt", "f\n", "F");
    pusher.push(None, false, &quiet).unwrap();
    commit(&b, "g.txt", "g\n", "G");
    assert_eq!(puller.pull(PullMode::Merge, &quiet).unwrap(), PullResult::Pulled);
    assert_eq!(b.sh(&["rev-list", "--count", "--merges", "HEAD"]).trim(), "1");

    // A conflict: left for the conflict view, where aborting puts everything back (tests/undo.rs resolves one).
    puller.push(None, false, &quiet).unwrap();
    a.sh(&["pull", "-q", "--ff-only"]);
    commit(&a, "b.txt", "a's b\n", "A's b");
    pusher.push(None, false, &quiet).unwrap();
    commit(&b, "b.txt", "b's b\n", "B's b");
    let before = head(&b);
    assert_eq!(puller.pull(PullMode::Merge, &quiet).unwrap(), PullResult::Conflicts);
    assert!(b.path(".git/MERGE_HEAD").exists());
    puller.abort().unwrap();
    assert_eq!(head(&b), before);
    assert!(b.sh(&["status", "--porcelain"]).trim().is_empty());
    assert_eq!(puller.pull(PullMode::Rebase, &quiet).unwrap(), PullResult::Conflicts);
    assert!(crate::said::english(&puller.operation().unwrap().unwrap().title).starts_with("Rebasing main"));
    puller.abort().unwrap();
    assert_eq!(head(&b), before);
    assert!(!b.path(".git/rebase-merge").exists() && !b.path(".git/rebase-apply").exists());
}

#[test]
fn says_what_a_pull_would_bring_in() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();
    let b = clone(&remote);
    commit(&a, "c.txt", "c\n", "C");
    commit(&a, "d.txt", "d\n", "D");
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();
    commit(&b, "e.txt", "e\n", "Mine");
    Writer::new(&b.root, true).fetch(&quiet).unwrap();

    let incoming = crate::git::incoming(&b.root).unwrap().unwrap();
    assert_eq!(incoming.upstream, "origin/main");
    assert_eq!(incoming.commits.iter().map(|commit| commit.subject.as_str()).collect::<Vec<_>>(), ["D", "C"]);
    assert_eq!(incoming.ahead, 1);
    assert_eq!(incoming.tip, head(&a));
    assert_eq!(crate::git::count_between(&b.root, &incoming.base, &incoming.tip), 2);
    // A branch that tracks nothing has nothing incoming.
    b.sh(&["checkout", "-q", "-b", "alone"]);
    assert!(crate::git::incoming(&b.root).unwrap().is_none());
}

#[test]
fn a_background_fetch_runs_beside_your_writes_and_skips_while_another_fetch_runs() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();
    let b = clone(&remote);
    commit(&a, "c.txt", "c\n", "C");
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();

    // Another network run holds the repository: the background fetch skips rather than waits.
    let network = crate::git::write::network::network_turn(&b.root);
    let held = network.lock();
    assert!(Writer::new(&b.root, false).fetch_in_background().is_none());
    drop(held);
    // A write (staging, a commit) holds the write queue: the background fetch runs beside it, and the write never
    // waits for the fetch.
    let queue = crate::git::write::queue(&b.root);
    let held = queue.lock();
    assert_eq!(Writer::new(&b.root, false).fetch_in_background(), Some(Ok(())));
    drop(held);
    assert_eq!(b.sh(&["rev-parse", "origin/main"]).trim(), head(&a));
    // No remote: nothing to do.
    assert!(Writer::new(&repo().root, true).fetch_in_background().is_none());
}

#[test]
fn pushes_a_tag_and_leaves_one_that_differs_on_the_remote() {
    let (_dir, remote) = bare();
    let r = repo();
    r.sh(&["remote", "add", "origin", &remote]);
    let writer = Writer::new(&r.root, true);
    writer.push(None, false, &quiet).unwrap();
    r.sh(&["tag", "v1"]);
    assert_eq!(writer.push_tag("v1", None, &quiet).unwrap(), PushResult::Pushed);
    assert_eq!(remote_sha(&remote, "v1"), head(&r));
    commit(&r, "a.txt", "later\n", "Later");
    r.sh(&["tag", "-f", "v1"]);
    assert!(writer.push_tag("v1", None, &quiet).unwrap_err().contains("already has a tag v1"));
    assert!(writer.push_tag("nope", None, &quiet).is_err());
}

#[test]
fn an_untrusted_folder_fetches_without_running_its_own_config_and_pulls_or_pushes_nothing() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();
    let b = clone(&remote);
    commit(&a, "c.txt", "c\n", "C");
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();

    let outside = tempfile::tempdir().unwrap();
    let marker = |name: &str| outside.path().join(name).to_string_lossy().replace('\\', "/");
    // A reference-transaction hook and an upload-pack command of the repository's own.
    let hook = b.path(".git/hooks/reference-transaction");
    fs::write(&hook, format!("#!/bin/sh\necho ran > \"{}\"\n", marker("hook"))).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    b.sh(&["config", "remote.origin.uploadpack", &format!("echo ran > \"{}\"; git-upload-pack", marker("upload"))]);

    let untrusted = Writer::new(&b.root, false);
    untrusted.fetch(&quiet).unwrap();
    assert_eq!(b.sh(&["rev-parse", "origin/main"]).trim(), head(&a), "the fetch itself worked");
    for name in ["hook", "upload"] {
        assert!(!outside.path().join(name).exists(), "an untrusted fetch ran the repository's {name}");
    }
    assert!(untrusted.pull(PullMode::FastForward, &quiet).unwrap_err().contains("trusted"));
    assert!(untrusted.push(None, false, &quiet).unwrap_err().contains("trusted"));

    // Trusted, the same fetch runs them: the markers above prove something.
    commit(&a, "d.txt", "d\n", "D");
    Writer::new(&a.root, true).push(None, false, &quiet).unwrap();
    Writer::new(&b.root, true).fetch(&quiet).unwrap();
    for name in ["hook", "upload"] {
        assert!(outside.path().join(name).exists(), "a trusted fetch should run the repository's {name}");
    }
}

#[test]
fn an_untrusted_fetch_refuses_the_ext_transport_and_the_repositorys_ssh_command() {
    let r = repo();
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran").to_string_lossy().replace('\\', "/");
    r.sh(&["remote", "add", "ext", &format!("ext::sh -c \"echo ran > '{marker}'\"")]);
    r.sh(&["remote", "add", "ssh", "ssh://git@example.invalid/x.git"]);
    r.sh(&["config", "core.sshCommand", &format!("sh -c 'echo ran > \"{marker}\"'")]);
    assert!(Writer::new(&r.root, false).fetch(&quiet).is_err());
    assert!(!std::path::Path::new(&marker).exists(), "an untrusted fetch ran the repository's own command");
}

#[test]
fn a_question_without_haunt_listening_stops_git_instead_of_hanging() {
    // No askpass and no terminal: git cannot ask for a password and stops with its own message.
    let r = repo();
    r.sh(&["remote", "add", "origin", "https://127.0.0.1:9/x.git"]);
    assert!(Writer::new(&r.root, true).fetch(&quiet).is_err());
}

#[test]
fn an_untrusted_fetch_asks_your_credential_helpers_and_not_the_repositorys() {
    let r = repo();
    let outside = tempfile::tempdir().unwrap();
    let marker = |name: &str| outside.path().join(name).to_string_lossy().replace('\\', "/");
    let global = outside.path().join("global.gitconfig");
    fs::write(&global, format!("[credential]\n\thelper = \"!echo ran > '{}'\"\n", marker("yours"))).unwrap();
    r.sh(&["config", "credential.helper", &format!("!echo ran > '{}'", marker("theirs"))]);
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&r.root)
            // Your helpers here are the global file's alone: a system one (Git Credential Manager) would answer first.
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let listing = git(&["config", "--list", "--show-scope", "--includes", "-z"]).wait_with_output().unwrap();
    let (config, _) = crate::git::write::network::untrusted_fetch(&crate::git::write::network::settings(
        &String::from_utf8_lossy(&listing.stdout),
    ));
    let mut args: Vec<String> = config.into_iter().flat_map(|setting| ["-c".to_string(), setting]).collect();
    args.extend(["credential".into(), "fill".into()]);
    let mut child = git(&args.iter().map(String::as_str).collect::<Vec<_>>());
    use std::io::Write as _;
    child.stdin.take().unwrap().write_all(b"protocol=https\nhost=example.com\n\n").unwrap();
    let _ = child.wait_with_output();
    assert!(outside.path().join("yours").exists(), "your own helper should be asked");
    assert!(!outside.path().join("theirs").exists(), "the repository's own helper ran");
}

#[test]
fn fetches_pull_requests_as_refs_and_reads_them_like_branches() {
    let (_dir, remote) = bare();
    let a = repo();
    a.sh(&["remote", "add", "origin", &remote]);
    a.sh(&["push", "-q", "origin", "main"]);
    // Two pull requests on the remote, as GitHub keeps them: refs/pull/<n>/head.
    a.sh(&["switch", "-q", "-c", "work"]);
    commit(&a, "one.txt", "1\n", "First change");
    a.sh(&["push", "-q", "origin", "HEAD:refs/pull/7/head"]);
    commit(&a, "two.txt", "2\n", "Second change");
    a.sh(&["push", "-q", "origin", "HEAD:refs/pull/12/head"]);

    let b = clone(&remote);
    b.sh(&["remote", "set-head", "origin", "main"]);
    // An untrusted folder may fetch them too: a fetch runs nothing of the repository's own.
    let writer = Writer::new(&b.root, false);
    let fetched = writer.fetch_pull_requests(None, None, &quiet).unwrap();
    assert_eq!((fetched.remote.as_str(), fetched.total, fetched.fetched), ("origin", 2, 2));
    // Not a branch: the branch lists and the commit list's refs do not show them.
    assert!(!b.sh(&["branch", "-a"]).contains("pull"));

    let pulls = crate::git::pull_requests(&b.root).unwrap();
    let seen: Vec<(u32, &str, u32)> =
        pulls.iter().map(|pull| (pull.number, pull.subject.as_str(), pull.commits)).collect();
    assert_eq!(seen, [(12, "Second change", 2), (7, "First change", 1)]);
    assert_eq!(pulls[0].base_label.as_deref(), Some("origin/main"));
    let commits = crate::git::pull_commits(&b.root, pulls[0].base.as_deref(), &pulls[0].sha).unwrap();
    assert_eq!(
        commits.iter().map(|commit| commit.subject.as_str()).collect::<Vec<_>>(),
        ["Second change", "First change"]
    );

    let missing = writer.fetch_pull_requests(Some("origin"), Some(99), &quiet).unwrap_err();
    assert!(missing.contains("#99"), "{missing}");
    assert!(writer.fetch_pull_requests(Some("upstream"), None, &quiet).is_err());
}

#[test]
fn lists_submodules_and_updates_them_only_when_trusted() {
    // git clones submodules from a folder on disk only when allowed, and only from your own config: here, the test's
    // environment (harmless for the tests running beside it).
    std::env::set_var("GIT_CONFIG_COUNT", "1");
    std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
    std::env::set_var("GIT_CONFIG_VALUE_0", "always");
    // A library repository, and a project that has it as a submodule at libs/lib.
    let lib = Repo::new();
    commit(&lib, "lib.txt", "1\n", "Lib one");
    let lib_path = lib.root.replace('\\', "/");
    let project = Repo::new();
    commit(&project, "app.txt", "app\n", "First");
    project.sh(&["submodule", "add", "-q", &lib_path, "libs/lib"]);
    project.sh(&["commit", "-q", "-m", "Add lib"]);
    let project_path = project.root.replace('\\', "/");

    // A clone without its submodules: listed, not checked out.
    let r = Repo::new();
    r.sh(&["remote", "add", "origin", &project_path]);
    r.sh(&["fetch", "-q", "origin"]);
    r.sh(&["reset", "-q", "--hard", "origin/main"]);
    let listed = crate::git::submodules(&r.root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].path.as_str(), listed[0].name.as_str()), ("libs/lib", "libs/lib"));
    assert_eq!(listed[0].url, lib_path);
    assert_eq!(listed[0].state, crate::git::submodules::SubmoduleState::Missing);

    let refused = Writer::new(&r.root, false).update_submodules(&[], &quiet).unwrap_err();
    assert!(refused.contains("trusted"), "{refused}");
    assert!(Writer::new(&r.root, true).update_submodules(&["../out".into()], &quiet).is_err());
    Writer::new(&r.root, true).update_submodules(&[], &quiet).unwrap();
    let listed = crate::git::submodules(&r.root).unwrap();
    assert_eq!(listed[0].state, crate::git::submodules::SubmoduleState::Current);
    assert_eq!(listed[0].checked_out.as_deref(), Some(listed[0].recorded.as_str()));

    // Another commit checked out inside it: moved, until updated again.
    commit(&lib, "lib.txt", "2\n", "Lib two");
    let inside = std::path::Path::new(&r.root).join("libs/lib").to_string_lossy().into_owned();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(&inside).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["fetch", "-q", "origin"]);
    run(&["checkout", "-q", "origin/main"]);
    assert_eq!(crate::git::submodules(&r.root).unwrap()[0].state, crate::git::submodules::SubmoduleState::Moved);
    Writer::new(&r.root, true).update_submodules(&["libs/lib".into()], &quiet).unwrap();
    assert_eq!(crate::git::submodules(&r.root).unwrap()[0].state, crate::git::submodules::SubmoduleState::Current);
}
