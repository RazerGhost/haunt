//! Remotes: add, rename, remove, and change a remote's URL, under the rules of `write/mod.rs`. They change only the
//! repository's config (and, renaming or removing, its remote branches): no hook or filter runs, so a folder does not
//! need trusting for them.
//!
//! - Names are checked as git checks them (`refs/remotes/<name>/…` must be a valid ref) and never start with a dash;
//!   names and URLs follow `--end-of-options`.
//! - A URL may not start with a dash, and git's `ext::` transport, which runs a command, is refused.

use super::{queue, Writer};
use crate::git::cli::git as read;

impl Writer<'_> {
    fn remote_name<'n>(&self, name: &'n str) -> Result<&'n str, String> {
        let bad = name.is_empty() || name.starts_with('-') || name.contains(char::is_whitespace);
        if bad || read(self.root, &["check-ref-format", &format!("refs/remotes/{name}/HEAD")]).is_err() {
            return Err(said!("remotes.badName", "\"{name}\" cannot be a remote's name.", name = name));
        }
        Ok(name)
    }

    fn known_remote(&self, name: &str) -> Result<(), String> {
        let remotes = read(self.root, &["remote"])?;
        if remotes.lines().any(|known| known.trim() == name) {
            Ok(())
        } else {
            Err(said!("remotes.none", "This repository has no remote \"{name}\".", name = name))
        }
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), String> {
        let name = self.remote_name(name.trim())?;
        let url = remote_url(url)?;
        if self.known_remote(name).is_ok() {
            return Err(said!("remotes.there", "This repository has a remote \"{name}\" already.", name = name));
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.git(&["remote", "add", "--end-of-options", name, url], None, &[], None).map(drop)
    }

    /// Renames a remote, with its remote branches and the branches that track them.
    pub fn rename_remote(&self, from: &str, to: &str) -> Result<(), String> {
        self.known_remote(from)?;
        let to = self.remote_name(to.trim())?;
        if from == to {
            return Ok(());
        }
        if self.known_remote(to).is_ok() {
            return Err(said!("remotes.there", "This repository has a remote \"{name}\" already.", name = to));
        }
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.git(&["remote", "rename", "--end-of-options", from, to], None, &[], None).map(drop)
    }

    /// Removes a remote and its remote branches; branches that tracked it track nothing after.
    pub fn remove_remote(&self, name: &str) -> Result<(), String> {
        self.known_remote(name)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.git(&["remote", "remove", "--end-of-options", name], None, &[], None).map(drop)
    }

    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<(), String> {
        self.known_remote(name)?;
        let url = remote_url(url)?;
        let queue = queue(self.root);
        let _turn = queue.lock().unwrap();
        self.git(&["remote", "set-url", "--end-of-options", name, url], None, &[], None).map(drop)
    }
}

/// A remote's URL from the window: not empty, not an option, not git's command-running `ext::` transport.
pub(super) fn remote_url(url: &str) -> Result<&str, String> {
    let url = url.trim();
    if url.is_empty() || url.starts_with('-') || url.contains(char::is_whitespace) {
        return Err(said!("remotes.badUrl", "That is not a URL git can use."));
    }
    if url.to_lowercase().starts_with("ext::") {
        return Err(said!("remotes.ext", "Haunt does not use ext:: URLs: they run a command."));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_urls_git_can_use_and_refuses_the_others() {
        assert_eq!(remote_url(" git@github.com:o/r.git "), Ok("git@github.com:o/r.git"));
        assert!(remote_url("https://example.com/o/r.git").is_ok());
        assert!(remote_url("").is_err());
        assert!(remote_url("--upload-pack=touch x").is_err());
        assert!(remote_url("ext::sh -c touch% x").is_err());
        assert!(remote_url("EXT::ssh").is_err());
    }
}
