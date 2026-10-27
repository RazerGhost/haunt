//! What the window gets from git, in one place. Each type is sent as JSON with camelCase names, and its TypeScript twin
//! is generated from here (specta), so the two cannot drift apart.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A branch, remote branch or tag pointing at a commit, or HEAD.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct Decoration {
    pub kind: DecorationKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum DecorationKind {
    Head,
    Branch,
    Remote,
    Tag,
}

/// A row of the commit list.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LogCommit {
    pub sha: String,
    /// As history simplification rewrote them, when the list is filtered by path.
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub refs: Vec<Decoration>,
    /// It carries a signature (GPG, SSH or X.509), checked or not; the commit's details check it.
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct LogPage {
    pub commits: Vec<LogCommit>,
    /// More commits follow: ask again with `skip` = how many were read so far.
    pub more: bool,
}

/// Which commits the list keeps by their parents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum MergeFilter {
    /// Only merge commits (two or more parents).
    Only,
    /// No merge commits.
    None,
}

/// What the commit list asks for.
#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(default)]
pub struct LogQuery {
    /// Branches, tags or commits to list from; none lists every branch, remote branch and tag, and HEAD.
    pub from: Option<Vec<String>>,
    pub skip: Option<u32>,
    pub limit: Option<u32>,
    /// Only commits whose author's name or email contains this (any case).
    pub author: Option<String>,
    /// Only commits whose message contains this (any case).
    pub message: Option<String>,
    /// Only commits that touch this file or folder.
    pub path: Option<String>,
    /// Only commits that add or remove this text (`git log -S`, as it is written).
    pub changed: Option<String>,
    /// Only merge commits, or none of them; all when null.
    pub merges: Option<MergeFilter>,
    /// Newest first by date, each branch's commits among the others', instead of each merged
    /// branch's commits together under its merge; more lanes stay open side by side.
    pub by_date: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum PathStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Type,
}

/// A file a commit (or two commits compared) changes.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangedPath {
    pub path: String,
    pub old_path: String,
    pub status: PathStatus,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
}

/// A commit with its message and the files it changes (against its first parent).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub committer: String,
    pub committer_time: i64,
    pub subject: String,
    /// The message after the subject.
    pub body: String,
    pub refs: Vec<Decoration>,
    pub files: Vec<ChangedPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line numbers on the old and the new side (null on the side that does not have the line).
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// The function the hunk is in, as git finds it: the nearest line above it that starts a definition.
    pub section: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffView {
    pub path: String,
    pub old_path: String,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
}

/// A local branch, with how far it is from the branch it tracks.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BranchRef {
    pub name: String,
    pub sha: String,
    pub current: bool,
    /// The branch it tracks (`origin/main`), or null.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// It tracks a branch that no longer exists on the remote.
    pub gone: bool,
    pub subject: String,
    pub time: i64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RemoteBranchRef {
    pub name: String,
    pub sha: String,
    pub subject: String,
    pub time: i64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct Remote {
    pub name: String,
    pub branches: Vec<RemoteBranchRef>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TagRef {
    pub name: String,
    pub sha: String,
    pub time: i64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct StashRef {
    pub index: u32,
    /// Its commit: what it saved, against the commit it was made on (its first parent).
    pub sha: String,
    pub message: String,
    pub time: i64,
}

/// The repository's refs, for the sidebar: what `git branch`, `git tag` and `git stash list` show.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Refs {
    pub branch: Option<String>,
    pub sha: Option<String>,
    pub branches: Vec<BranchRef>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<TagRef>,
    pub stashes: Vec<StashRef>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CommitInfo {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

/// The `git log --format` a `CommitInfo` is read from (`CommitInfo::parse`): sha, author, time and subject.
pub const COMMIT_FORMAT: &str = "%H%x1f%an%x1f%at%x1f%s";

impl CommitInfo {
    /// A line of `COMMIT_FORMAT`; None for an empty one.
    pub fn parse(line: &str) -> Option<CommitInfo> {
        let mut parts = line.trim_end_matches(['\r', '\n']).split('\x1f');
        let mut next = || parts.next().unwrap_or_default().to_owned();
        let (sha, author, time, subject) = (next(), next(), next(), next());
        (!sha.is_empty()).then(|| CommitInfo { sha, author, time: time.parse().unwrap_or(0), subject })
    }

    /// Every line of `COMMIT_FORMAT` output.
    pub fn all(out: &str) -> Vec<CommitInfo> {
        out.lines().filter_map(CommitInfo::parse).collect()
    }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AuthorLines {
    pub name: String,
    pub lines: u32,
}

/// One commit of a file's history: the commit, its first parent, and the file's path there (and before, for a rename).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileVersion {
    pub commit: CommitInfo,
    pub parent: Option<String>,
    pub path: String,
    pub old_path: String,
    pub status: PathStatus,
}

/// A page of a file's history, newest first.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FileHistory {
    pub versions: Vec<FileVersion>,
    /// There are older commits after this page.
    pub more: bool,
}

/// A line of a file with the commit that last changed it ("0000…" for a change not committed yet).
#[derive(Debug, Clone, Serialize, Type)]
pub struct BlameLine {
    pub sha: String,
    pub text: String,
}

/// A whole file with who changed each line last.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FileBlame {
    pub lines: Vec<BlameLine>,
    /// The commits the lines name, once each.
    pub commits: Vec<CommitInfo>,
    /// Only the first lines are here: the file is longer.
    pub cut: bool,
}

/// A function's history: the commits that touched its lines, and who wrote them.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FunctionHistory {
    pub commits: Vec<CommitInfo>,
    pub authors: Vec<AuthorLines>,
    pub latest: Option<CommitInfo>,
    /// Where it was moved or renamed into another file, newest first: its history goes on from there.
    pub moves: Vec<HistoryMove>,
    /// The commits whose change to it was only cosmetic (spacing, blank lines, comments).
    pub cosmetic: Vec<String>,
    /// The history reaches back to where it began: its last commit added it.
    pub complete: bool,
}

/// A commit that moved a function here from another file or name (`code/changes.rs` pairs them).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMove {
    pub sha: String,
    pub from_path: String,
    pub from_name: String,
    pub to_path: String,
    pub to_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Type)]
pub struct Churn {
    /// Commits that touched the file in the period.
    pub commits: u32,
    /// Lines added and removed in them.
    pub lines: u32,
}

/// How much each file changed in a period, by commits and lines.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Hotspots {
    pub days: u32,
    pub files: BTreeMap<String, Churn>,
}
