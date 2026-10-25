//! Interactive rebase: the commits after a base, reordered, reworded, squashed, fixed up and dropped without an editor;
//! one that stops on conflicts, continued; and the refusals (a merge in the range, a changed list, squashing first).

use super::repo;
use crate::git::tests::Repo;
use crate::git::write::rebase::RebaseAction;
use crate::git::write::{Outcome, RebaseStep, Writer};

fn commit(r: &Repo, path: &str, text: &str, message: &str) -> String {
    r.write(path, text);
    r.sh(&["add", path]);
    r.sh(&["commit", "-q", "-m", message]);
    r.sh(&["rev-parse", "HEAD"]).trim().to_owned()
}

fn subjects(r: &Repo, range: &str) -> Vec<String> {
    r.sh(&["log", "--reverse", "--format=%s", range]).lines().map(str::to_owned).collect()
}

fn step(sha: &str, action: RebaseAction) -> RebaseStep {
    RebaseStep { sha: sha.into(), action, message: None }
}

#[test]
fn reorders_rewords_squashes_fixes_up_and_drops() {
    let r = repo();
    let base = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
    let one = commit(&r, "one.txt", "1\n", "One");
    let two = commit(&r, "two.txt", "2\n", "Two");
    let three = commit(&r, "three.txt", "3\n", "Three");
    let four = commit(&r, "four.txt", "4\n", "Four");
    let five = commit(&r, "five.txt", "5\n", "Five");
    let writer = Writer::new(&r.root, true);
    let plan = writer.rebase_plan(&base).unwrap();
    assert_eq!(
        plan.iter().map(|commit| commit.subject.as_str()).collect::<Vec<_>>(),
        ["One", "Two", "Three", "Four", "Five"]
    );

    let steps = vec![
        step(&three, RebaseAction::Pick),
        RebaseStep { sha: one.clone(), action: RebaseAction::Reword, message: Some("One, reworded".into()) },
        step(&two, RebaseAction::Squash),
        step(&four, RebaseAction::Drop),
        step(&five, RebaseAction::Fixup),
    ];
    assert_eq!(writer.rebase(&base, &steps).unwrap(), Outcome::Done);
    let range = format!("{base}..HEAD");
    // Five went into the squashed commit as a fixup; the squash kept both messages.
    assert_eq!(subjects(&r, &range), ["Three", "One, reworded"]);
    let message = r.sh(&["log", "-1", "--format=%B"]);
    assert!(message.contains("One, reworded") && message.contains("Two"), "{message}");
    assert!(!message.contains("Five"));
    assert!(r.path("five.txt").exists() && !r.path("four.txt").exists());
    assert!(writer.operation().unwrap().is_none());
}

#[test]
fn refuses_a_changed_list_a_first_squash_and_a_merge_in_the_range() {
    let r = repo();
    let base = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
    let one = commit(&r, "one.txt", "1\n", "One");
    let two = commit(&r, "two.txt", "2\n", "Two");
    let writer = Writer::new(&r.root, true);
    assert!(writer.rebase(&base, &[step(&one, RebaseAction::Pick)]).unwrap_err().contains("changed"));
    let squash_first = [step(&one, RebaseAction::Squash), step(&two, RebaseAction::Pick)];
    assert!(writer.rebase(&base, &squash_first).unwrap_err().contains("cannot be squashed"));
    let all_dropped = [step(&one, RebaseAction::Drop), step(&two, RebaseAction::Drop)];
    assert!(writer.rebase(&base, &all_dropped).is_err());
    let no_message = [step(&one, RebaseAction::Reword), step(&two, RebaseAction::Pick)];
    assert!(writer.rebase(&base, &no_message).unwrap_err().contains("message"));

    r.sh(&["checkout", "-q", "-b", "side", &base]);
    commit(&r, "side.txt", "s\n", "Side");
    r.sh(&["checkout", "-q", "main"]);
    r.sh(&["merge", "-q", "--no-edit", "side"]);
    assert!(writer.rebase_plan(&base).unwrap_err().contains("merge"));
}

#[test]
fn a_rebase_that_conflicts_is_continued_from_the_conflict_view() {
    let r = repo();
    let base = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
    let first = commit(&r, "b.txt", "first\n", "First change of b");
    let second = commit(&r, "b.txt", "second\n", "Second change of b");
    let writer = Writer::new(&r.root, true);
    // Swapped, the second change no longer applies on the base: a conflict.
    let steps = [step(&second, RebaseAction::Pick), step(&first, RebaseAction::Pick)];
    assert_eq!(writer.rebase(&base, &steps).unwrap(), Outcome::Conflicts);
    let going = writer.operation().unwrap().unwrap();
    assert!(going.title.starts_with("Rebasing main"), "{}", going.title);
    writer.resolve("b.txt", Some("second\n")).unwrap();
    let mut outcome = writer.proceed().unwrap();
    // The first change now conflicts with the second one in turn.
    while outcome == Outcome::Conflicts {
        writer.resolve("b.txt", Some("first\n")).unwrap();
        outcome = writer.proceed().unwrap();
    }
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(subjects(&r, &format!("{base}..HEAD")), ["Second change of b", "First change of b"]);
}
