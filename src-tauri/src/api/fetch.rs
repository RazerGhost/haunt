//! The background fetch, and when each repository was last fetched (by you or in the background).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use specta::Type;

use super::Api;
use crate::git::write::Writer;

/// Fetches the repository shown every so many minutes when Settings asks for it (Git: fetch in the background), so the
/// counts on Pull and Push stay current. Quietly: it asks nothing, waits for nothing and says nothing when it fails;
/// the window hears only that the refs changed. Each repository is fetched at most once per interval.
pub fn fetch_in_background(api: Arc<Api>) {
    std::thread::spawn(move || {
        let mut last: HashMap<String, std::time::Instant> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let (every, Some(root)) = api.repos.fetch_schedule() else { continue };
            let due = last.get(&root).is_none_or(|at| at.elapsed() >= Duration::from_secs(u64::from(every) * 60));
            if every == 0 || !due {
                continue;
            }
            last.insert(root.clone(), std::time::Instant::now());
            let Ok(trusted) = api.repos.trusted(&root) else { continue };
            let before = crate::git::upstream_tip(&root);
            let Some(result) = Writer::new(&root, trusted).fetch_in_background() else { continue };
            record_fetch(&root, &result, true);
            api.repos.announce_git(&root, &["refs"]);
            // New commits on the branch's upstream: the window says so (one click from seeing them).
            if let (Ok(()), Some(before), Some(after)) = (&result, before, crate::git::upstream_tip(&root)) {
                let commits = crate::git::count_between(&root, &before, &after);
                if commits > 0 {
                    api.repos.news(&root, serde_json::json!({ "type": "fetched", "commits": commits }));
                }
            }
        }
    });
}

/// When a repository was last fetched, and how it went.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LastFetch {
    /// Seconds since 1970.
    pub at: i64,
    pub ok: bool,
    /// What git said when it failed.
    pub message: Option<String>,
    /// A background fetch, not one you asked for.
    pub background: bool,
}

pub(super) fn fetches() -> &'static parking_lot::Mutex<HashMap<String, LastFetch>> {
    static FETCHES: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, LastFetch>>> = std::sync::OnceLock::new();
    FETCHES.get_or_init(Default::default)
}

pub(super) fn record_fetch(root: &str, result: &Result<(), String>, background: bool) {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0);
    let entry = LastFetch { at, ok: result.is_ok(), message: result.as_ref().err().cloned(), background };
    fetches().lock().insert(root.to_owned(), entry);
}
