//! Bisecting: finding the first bad commit between a good one and a bad one by testing commits in between (`git
//! bisect`), step by step from the window. Under the rules of `write/mod.rs`; every step checks a commit out, so in an
//! untrusted folder whose files go through filters it is refused as switching branches is. Any commit still in play can
//! be tested instead of the one git picked (one that touched the function that misbehaves, say).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{queue, Writer};
use crate::git::cli::{git as read, revision};
use crate::git::types::{CommitInfo, COMMIT_FORMAT};

/// The most commits still in play the state lists.
const LISTED: usize = 500;

/// How a tested commit turned out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum BisectMark {
    Good,
    Bad,
    /// Cannot be tested (it does not build, say): git picks another.
    Skip,
}

/// Where a bisect stands.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BisectState {
    pub active: bool,
    pub bad: Option<String>,
    pub goods: Vec<String>,
    pub skipped: Vec<String>,
    /// The commit checked out now, to test.
    pub current: Option<CommitInfo>,
    /// The commits that may still be the first bad one (skipped ones left out), newest first; at most `LISTED`.
    pub candidates: Vec<String>,
    /// How many there are, and about how many steps are left.
    pub left: u32,
    pub steps: u32,
    /// Found: the only commit left.
    pub culprit: Option<CommitInfo>,
}

fn info(root: &str, sha: &str) -> Option<CommitInfo> {
    let out = read(root, &["log", "-1", &format!("--format={COMMIT_FORMAT}"), sha]).ok()?;
    CommitInfo::parse(out.trim())
}

/// The commits of the refs under `refs/bisect/` whose names start with `prefix` (`for-each-ref` matches whole path
/// segments only, so they are filtered here).
fn refs_under(root: &str, prefix: &str) -> Vec<String> {
    read(root, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/bisect/"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(_, sha)| sha.to_owned())
        .collect()
}

pub fn bisect_state(root: &str) -> BisectState {
    let start = read(root, &["rev-parse", "--path-format=absolute", "--git-path", "BISECT_START"]).unwrap_or_default();
    let active = !start.trim().is_empty() && std::path::Path::new(start.trim()).exists();
    let mut state = BisectState {
        active,
        bad: None,
        goods: Vec::new(),
        skipped: Vec::new(),
        current: None,
        candidates: Vec::new(),
        left: 0,
        steps: 0,
        culprit: None,
    };
    if !active {
        return state;
    }
    state.bad = read(root, &["rev-parse", "--verify", "--quiet", "refs/bisect/bad"])
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|sha| !sha.is_empty());
    state.goods = refs_under(root, "refs/bisect/good-");
    state.skipped = refs_under(root, "refs/bisect/skip-");
    state.current = info(root, "HEAD");
    if let Some(bad) = &state.bad {
        if !state.goods.is_empty() {
            let mut args: Vec<String> = vec!["rev-list".into(), bad.clone(), "--not".into()];
            args.extend(state.goods.iter().cloned());
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let all: Vec<String> = read(root, &args)
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.is_empty() && !state.skipped.iter().any(|skip| skip == line))
                .map(str::to_owned)
                .collect();
            state.left = all.len() as u32;
            state.steps = if all.len() > 1 { (all.len() as f64).log2().ceil() as u32 } else { 0 };
            if all.len() == 1 {
                state.culprit = info(root, &all[0]);
            }
            state.candidates = all.into_iter().take(LISTED).collect();
        }
    }
    state
}

impl Writer<'_> {
    /// Starts bisecting between `bad` and `good` (branches, tags or commits): git checks out a commit in between.
    pub fn bisect_start(&self, bad: &str, good: &str) -> Result<(), String> {
        let bad = revision(bad)?;
        let good = revision(good)?;
        self.refuse_checkout(super::refs::Checkout::Bisect)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        // `bisect start` takes no --end-of-options; `revision` already refused anything starting with a dash.
        self.git(&["bisect", "start", bad, good, "--"], None, &[], None).map(drop)
    }

    /// Marks the commit checked out as good, bad or one to skip; git checks out the next one to test.
    pub fn bisect_mark(&self, mark: BisectMark) -> Result<(), String> {
        let term = match mark {
            BisectMark::Good => "good",
            BisectMark::Bad => "bad",
            BisectMark::Skip => "skip",
        };
        self.refuse_checkout(super::refs::Checkout::Bisect)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        self.git(&["bisect", term], None, &[], None).map(drop)
    }

    /// Tests another commit still in play instead of the one git picked.
    pub fn bisect_jump(&self, sha: &str) -> Result<(), String> {
        let state = bisect_state(self.root);
        if !state.candidates.iter().any(|candidate| candidate == sha) {
            return Err(said!("bisect.notInPlay", "That commit is not one still in play."));
        }
        self.refuse_checkout(super::refs::Checkout::Bisect)?;
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        self.git(&["checkout", "--detach", "--end-of-options", sha], None, &[], None).map(drop)
    }

    /// Stops bisecting: back to the branch it started from.
    pub fn bisect_stop(&self) -> Result<(), String> {
        let queue = queue(self.root);
        let _turn = queue.lock();
        self.wait_for_lock()?;
        self.git(&["bisect", "reset"], None, &[], None).map(drop)
    }
}
