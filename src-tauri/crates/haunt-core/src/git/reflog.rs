//! The reflog: where HEAD, or a branch, pointed before, newest first, for getting back a commit or a branch tip that
//! a reset, a rebase, an amend or a deleted branch left behind. Each entry says what moved it (git's own words), and
//! whether its commit is still on a branch, tag, remote branch or stash, or lost: kept only by the reflog, until git
//! prunes it (90 days by default, 30 for what was never on a branch).

use serde::Serialize;
use specta::Type;

use super::cli::git;

/// At most this many entries are read.
const MOST: usize = 300;

#[derive(Debug, Clone, Serialize, Type)]
pub struct Reflog {
    /// "HEAD", or the branch's name.
    pub reference: String,
    pub entries: Vec<ReflogEntry>,
    /// There are older entries than these.
    pub more: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ReflogEntry {
    /// "HEAD@{3}": how git names it.
    pub selector: String,
    /// Where it pointed from this entry on.
    pub sha: String,
    /// When it moved there, in seconds.
    pub time: i64,
    /// What moved it, as git says it first: "commit", "checkout", "reset", "rebase (finish)", "merge main"…
    pub action: String,
    /// The rest of git's words: "moving from main to feature", the commit's subject…
    pub message: String,
    /// The subject of the commit it pointed at.
    pub subject: String,
    /// The commit is on no branch, tag, remote branch or stash: only the reflog keeps it.
    pub lost: bool,
}

/// The reflog of `reference`: "HEAD", or a local branch's name.
pub fn reflog(root: &str, reference: &str) -> Result<Reflog, String> {
    let full = if reference == "HEAD" {
        "HEAD".to_owned()
    } else {
        let name = super::cli::revision(reference)?;
        let full = format!("refs/heads/{name}");
        git(root, &["show-ref", "--verify", "--quiet", &full])
            .map_err(|_| said!("reflog.noBranch", "There is no branch \"{name}\".", name = name))?;
        full
    };
    let count = format!("--max-count={}", MOST + 1);
    // With --date=unix, %gd names each entry by its time: "HEAD@{1761580000}".
    let out =
        git(root, &["log", "-g", &count, "--date=unix", "--format=%gd%x1f%H%x1f%gs%x1f%s", "--end-of-options", &full])
            .unwrap_or_default();
    let mut entries: Vec<ReflogEntry> = out.lines().filter_map(|line| parse(line, reference)).collect();
    let more = entries.len() > MOST;
    entries.truncate(MOST);
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.selector = format!("{reference}@{{{index}}}");
    }
    mark_lost(root, &mut entries);
    Ok(Reflog { reference: reference.to_owned(), entries, more })
}

/// One line of the log: "HEAD@{time} US sha US what moved it US subject".
fn parse(line: &str, reference: &str) -> Option<ReflogEntry> {
    let mut parts = line.split('\x1f');
    let selector = parts.next()?;
    let sha = parts.next()?.trim().to_owned();
    let said = parts.next().unwrap_or_default();
    let subject = parts.next().unwrap_or_default().to_owned();
    if sha.is_empty() {
        return None;
    }
    let time = selector.rsplit_once('{').and_then(|(_, rest)| rest.trim_end_matches('}').parse().ok()).unwrap_or(0);
    let (action, message) = match said.split_once(": ") {
        Some((action, message)) => (action.to_owned(), message.to_owned()),
        None => (said.to_owned(), String::new()),
    };
    Some(ReflogEntry { selector: reference.to_owned(), sha, time, action, message, subject, lost: false })
}

/// Marks the entries whose commit no ref reaches (Haunt's own saved discards and drops do not count), in one call:
/// `git name-rev` names each commit by a ref that reaches it, or "undefined".
fn mark_lost(root: &str, entries: &mut [ReflogEntry]) {
    let mut shas: Vec<&str> = Vec::new();
    for entry in entries.iter() {
        if !shas.contains(&entry.sha.as_str()) {
            shas.push(&entry.sha);
        }
    }
    if shas.is_empty() {
        return;
    }
    let mut args = vec!["name-rev", "--name-only", "--exclude=refs/haunt/*"];
    args.extend(shas.iter().copied());
    let Ok(out) = git(root, &args) else { return };
    let lost: Vec<String> = shas
        .iter()
        .zip(out.lines())
        .filter(|(_, name)| name.trim() == "undefined")
        .map(|(sha, _)| sha.to_string())
        .collect();
    for entry in entries.iter_mut() {
        entry.lost = lost.contains(&entry.sha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_moved_it_and_when() {
        let entry =
            parse("HEAD@{1761580000}\x1fabc\x1fcheckout: moving from main to feature\x1fAdd login", "HEAD").unwrap();
        assert_eq!(
            (entry.time, entry.action.as_str(), entry.message.as_str()),
            (1761580000, "checkout", "moving from main to feature")
        );
        assert_eq!((entry.sha.as_str(), entry.subject.as_str()), ("abc", "Add login"));
        let entry = parse("main@{1}\x1fdef\x1fbranch: Created from HEAD\x1f", "main").unwrap();
        assert_eq!(entry.action, "branch");
        assert!(parse("HEAD@{1}\x1f\x1fcommit: x\x1fx", "HEAD").is_none());
    }

    #[test]
    fn finds_a_commit_a_reset_left_behind() {
        let base = std::env::temp_dir().join(format!("haunt-reflog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let root = base.to_string_lossy().into_owned();
        let run = |args: &[&str]| {
            let mut command = std::process::Command::new("git");
            command
                .args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(&base);
            assert!(command.output().unwrap().status.success(), "{args:?}");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["commit", "-q", "--allow-empty", "-m", "first"]);
        run(&["commit", "-q", "--allow-empty", "-m", "second"]);
        run(&["reset", "-q", "--hard", "HEAD~1"]);
        let head = reflog(&root, "HEAD").unwrap();
        assert_eq!(
            head.entries.iter().map(|entry| entry.action.as_str()).collect::<Vec<_>>(),
            ["reset", "commit", "commit (initial)"]
        );
        assert_eq!(head.entries[1].selector, "HEAD@{1}");
        assert!(head.entries[1].lost, "the reset left \"second\" on no branch");
        assert!(!head.entries[0].lost);
        assert_eq!(reflog(&root, "main").unwrap().entries.len(), 3);
        assert!(reflog(&root, "gone").is_err());
        assert!(reflog(&root, "--all").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
