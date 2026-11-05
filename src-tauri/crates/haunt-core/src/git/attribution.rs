//! Which commits an AI helped write: a commit made with an AI assistant names it in a `Co-Authored-By:` trailer
//! (Claude Code writes `Co-Authored-By: Claude … <noreply@anthropic.com>`). The trailers of each commit are read once
//! (one `git log` for the commits not seen yet) and kept for as long as Haunt runs: a commit never changes. Which
//! co-authors count as an AI is the project's choice, in `haunt.config.json`:
//!
//! ```json
//! { "attribution": { "ai": ["claude", "anthropic.com", "copilot"] } }
//! ```
//!
//! each a piece of a co-author's name or email, in any case; without it, `claude` and `anthropic.com`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use serde_json::Value;

use super::cli::git;

/// What a co-author is matched against when the project names nothing.
pub const DEFAULT_AI: [&str; 2] = ["claude", "anthropic.com"];
/// How many commits one `git log` reads, to stay well inside a command line's length.
const PER_LOG: usize = 200;
/// How many commits' trailers are kept before the cache starts over.
const KEPT: usize = 200_000;

/// The co-authors a commit message names: the `Co-Authored-By` trailers of its last paragraph, in any case.
pub fn co_authors(message: &str) -> Vec<String> {
    let trimmed = message.trim_end();
    let last = trimmed.rsplit("\n\n").next().unwrap_or(trimmed);
    last.lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.trim().eq_ignore_ascii_case("co-authored-by"))
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

/// Whether a co-author (`Name <email>`) is an AI by the patterns: a pattern found in it, in any case.
pub fn is_ai(co_author: &str, patterns: &[String]) -> bool {
    let lower = co_author.to_lowercase();
    patterns.iter().any(|pattern| !pattern.is_empty() && lower.contains(&pattern.to_lowercase()))
}

/// The project's patterns: `attribution.ai` in its haunt.config.json, else the defaults. A config that cannot be read
/// or names no list keeps the defaults; an empty list counts no co-author as an AI.
pub fn ai_patterns(root: &str) -> Vec<String> {
    let defaults = || DEFAULT_AI.iter().map(|pattern| (*pattern).to_owned()).collect();
    let Ok(text) = std::fs::read_to_string(Path::new(root).join("haunt.config.json")) else { return defaults() };
    patterns_in(&text).unwrap_or_else(defaults)
}

fn patterns_in(config: &str) -> Option<Vec<String>> {
    let value: Value = serde_json::from_str(config).ok()?;
    let list = value.get("attribution")?.get("ai")?.as_array()?;
    Some(
        list.iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

type Trailers = HashMap<String, Arc<Vec<String>>>;

fn trailers() -> &'static Mutex<Trailers> {
    static TRAILERS: OnceLock<Mutex<Trailers>> = OnceLock::new();
    TRAILERS.get_or_init(Mutex::default)
}

/// The commits among `shas` (full hashes) that an AI co-wrote by `patterns`. Commits not read before are read with
/// one `git log` per few hundred; one that cannot be read counts as not.
pub fn ai_commits(root: &str, shas: &HashSet<String>, patterns: &[String]) -> HashSet<String> {
    let wanted: Vec<&String> = shas.iter().filter(|sha| is_sha(sha)).collect();
    let missing: Vec<&str> = {
        let known = trailers().lock();
        wanted.iter().filter(|sha| !known.contains_key(sha.as_str())).map(|sha| sha.as_str()).collect()
    };
    for chunk in missing.chunks(PER_LOG) {
        let mut args = vec!["log", "--no-walk=unsorted", "--format=%H%x1f%B%x1e"];
        args.extend(chunk.iter().copied());
        let Ok(out) = git(root, &args) else { continue };
        let mut known = trailers().lock();
        if known.len() > KEPT {
            known.clear();
        }
        for record in out.split('\x1e') {
            let Some((sha, message)) = record.trim_start_matches('\n').split_once('\x1f') else { continue };
            known.insert(sha.trim().to_owned(), Arc::new(co_authors(message)));
        }
    }
    let known = trailers().lock();
    wanted
        .into_iter()
        .filter(|sha| known.get(sha.as_str()).is_some_and(|names| names.iter().any(|name| is_ai(name, patterns))))
        .cloned()
        .collect()
}

/// A full commit hash, not the zeros blame gives lines not committed yet.
fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit()) && text.bytes().any(|byte| byte != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Repo;

    #[test]
    fn reads_the_co_authors_of_the_last_paragraph() {
        let message = "Split git into haunt-core\n\nCo-Authored-By: in the body is not a trailer\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\nco-authored-by: Ann <ann@example.com>\nSigned-off-by: Bo <bo@example.com>\n";
        assert_eq!(co_authors(message), ["Claude Opus 5.5 <noreply@anthropic.com>", "Ann <ann@example.com>"]);
        assert!(co_authors("Fix a typo").is_empty());
        assert!(co_authors("").is_empty());
    }

    #[test]
    fn matches_a_co_author_by_name_or_email_in_any_case() {
        let defaults: Vec<String> = DEFAULT_AI.iter().map(|pattern| (*pattern).to_owned()).collect();
        assert!(is_ai("Claude <noreply@example.com>", &defaults));
        assert!(is_ai("Assistant <bot@Anthropic.com>", &defaults));
        assert!(!is_ai("Ann <ann@example.com>", &defaults));
        assert!(!is_ai("Claude <noreply@anthropic.com>", &[]));
        assert!(!is_ai("Ann", &[String::new()]));
    }

    #[test]
    fn reads_the_patterns_from_the_config() {
        assert_eq!(
            patterns_in(r#"{ "attribution": { "ai": ["copilot", " Claude ", ""] } }"#).unwrap(),
            ["copilot", "Claude"]
        );
        assert_eq!(patterns_in(r#"{ "attribution": { "ai": [] } }"#).unwrap(), Vec::<String>::new());
        assert!(patterns_in(r#"{ "layers": [] }"#).is_none());
        assert!(patterns_in("not json").is_none());
        assert_eq!(ai_patterns("/no/such/folder"), DEFAULT_AI);
    }

    #[test]
    fn finds_the_commits_an_ai_co_wrote() {
        let r = Repo::new();
        r.write("a.txt", "one\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Alone"]);
        let alone = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        r.write("a.txt", "one\ntwo\n");
        r.sh(&["commit", "-q", "-am", "With help\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"]);
        let helped = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        let shas: HashSet<String> = [alone.clone(), helped.clone(), "0".repeat(40)].into_iter().collect();
        let patterns = ai_patterns(&r.root);
        assert_eq!(ai_commits(&r.root, &shas, &patterns), HashSet::from([helped.clone()]));
        // Read once: asked again, the answer comes from what was kept.
        assert!(trailers().lock().contains_key(&alone));
        assert_eq!(ai_commits(&r.root, &shas, &["copilot".to_owned()]), HashSet::new());
    }
}
