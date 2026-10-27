//! Git. The reads behind each click run in this process on gix (`objects.rs`); the big walks git itself does best,
//! through one safe runner (`cli.rs`): the log pages, refs and the status count, churn, blame and `log -L`. Writing
//! (staging, commits, branches, stashes and tags) is its own module, `write/`, with its own rules.

pub(crate) mod cli;
pub mod diff;
mod health;
mod history;
pub mod knowledge;
pub mod local;
mod log;
pub(crate) mod objects;
pub mod overview;
mod pulls;
pub mod reflog;
mod refs;
mod release;
mod remote;
mod submodules;
pub mod types;
pub mod write;

pub use cli::forget_config;
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
#[cfg(test)]
pub use remote::web_page;
pub use remote::{count_between, incoming, upstream_tip, web_remote, Incoming, WebKind, WebRemote};
pub use submodules::{submodules, Submodule};

#[cfg(test)]
pub(crate) mod tests;
