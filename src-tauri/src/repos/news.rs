//! What the window hears: news about the shown repository or about none, and git's changes Haunt made itself.

use std::time::{Duration, Instant};

use serde_json::json;

use super::Repos;

/// How long after Haunt's own write the watcher's news about the same parts counts as that write's.
const ANNOUNCED_FOR: Duration = Duration::from_millis(1500);

impl Repos {
    /// Sends the window news about the shown repository `repo`.
    pub fn news(&self, repo: &str, news: serde_json::Value) {
        self.events.send(Some(repo), news);
    }

    /// News that is about no tab (a clone's progress): the window hears it whatever it shows.
    pub fn news_for_all(&self, news: serde_json::Value) {
        self.events.send(None, news);
    }

    /// Tells the window something it could not do outside a repository's work (a dropped folder it cannot open).
    pub fn notice(&self, message: &str) {
        self.events.send(None, json!({ "type": "notice", "message": message }));
    }

    /// Tells the window that Haunt changed `parts` of git itself (a write), whether or not a watcher notices.
    pub fn announce_git(&self, repo: &str, parts: &[&str]) {
        let names = parts.iter().map(|part| part.to_string()).collect();
        self.announced.lock().insert(repo.to_owned(), (Instant::now(), names));
        self.events.send(Some(repo), json!({ "type": "git", "parts": parts }));
    }

    /// Whether the watcher's news about `parts` is Haunt's own last write, announced a moment ago already.
    pub(super) fn just_announced(&self, repo: &str, parts: &[&str]) -> bool {
        self.announced.lock().get(repo).is_some_and(|(at, announced)| {
            at.elapsed() < ANNOUNCED_FOR && parts.iter().all(|part| announced.iter().any(|known| known == part))
        })
    }
}
