//! Git. The reads behind each click run in this process on gix (`objects.rs`); the big walks git itself does best,
//! through one safe runner (`cli.rs`): the log pages, refs and the status count, churn, blame and `log -L`. Writing
//! (staging, commits, branches, stashes and tags) is its own module, `write/`, with its own rules.

pub mod cli;
pub mod command;
pub mod diff;
pub mod grep;
mod health;
mod history;
pub mod knowledge;
pub mod local;
mod log;
pub mod objects;
pub mod overview;
mod pulls;
pub mod reflog;
mod refs;
mod release;
mod remote;
pub mod signature;
mod submodules;
pub mod types;
pub mod write;

pub use cli::forget_config;
pub use cli::revision as cli_revision;
pub use health::{health, Health};
pub use history::{
    churn, file_blame, file_history, first_parent_changes, first_parents, function_history, last_mention, line_history,
    parent_and_size, HISTORY_COMMITS,
};
pub use log::log;
#[cfg(test)]
pub use objects::file_diff;
pub use objects::{commit, file_at, file_diff_with, files_between};
pub use pulls::{pull_commits, pull_requests, PullRequest};
pub use refs::{local_changes, merged_branches, refs};
pub use release::{release_commits, ReleaseCommit};
#[cfg(any(test, feature = "test-support"))]
pub use remote::web_page;
pub use remote::{
    count_between, incoming, pull_draft, remote_page, upstream_tip, web_remote, Incoming, PullDraft, WebKind, WebRemote,
};
pub use submodules::{submodules, Submodule};

#[cfg(any(test, feature = "test-support"))]
pub mod testing;
#[cfg(test)]
pub(crate) mod tests;
