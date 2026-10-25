//! Pull request status per branch, through the host's own command-line tool: `gh` for GitHub, `glab` for GitLab,
//! `tea` for Gitea (untested: its output is read as tea documents it, never tried against a real Gitea), picked by the
//! remote's host. Only when switched on in Settings (off by default); without the tool,
//! or signed out, nothing changes but a note. The tool runs in a folder of its own with the repository named on its
//! command line, so nothing of the repository (its config, its hooks) runs; it uses the account you signed in with.

use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use specta::Type;

use crate::git::{WebKind, WebRemote};

/// How long a tool may take.
const TIMEOUT: Duration = Duration::from_secs(20);
/// Reads a tool's answer.
type Parse = fn(&str) -> Result<Vec<HostPull>, String>;

/// How many pull requests are asked for.
const LISTED: &str = "40";

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostPull {
    pub number: u32,
    pub title: String,
    /// The branch it is from.
    pub branch: String,
    /// open, draft, merged or closed.
    pub state: String,
    /// passing, failing or pending; null when it has no checks, or the tool does not say.
    pub checks: Option<String>,
    /// approved, changes requested or review required; null when the tool does not say.
    pub review: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostPulls {
    /// The tool asked (gh, glab, tea); null when the remote's host has none, or it is not installed.
    pub tool: Option<String>,
    pub host: String,
    pub pulls: Vec<HostPull>,
    /// What went wrong (signed out, say), in words.
    pub problem: Option<String>,
}

/// Runs a tool in a folder of its own; None when it is not installed.
fn run(program: &str, args: &[&str]) -> Option<Result<String, String>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GLAB_NO_PROMPT", "1")
        .stdin(Stdio::null());
    let done = match crate::process::run(command, TIMEOUT) {
        Ok(done) => done,
        Err(crate::process::Failure::NotFound) => return None,
        Err(crate::process::Failure::TimedOut) => {
            return Some(Err(format!("{program} did not answer within {} seconds.", TIMEOUT.as_secs())))
        }
        Err(crate::process::Failure::Other(error)) => return Some(Err(error)),
    };
    let out = String::from_utf8_lossy(&done.stdout).into_owned();
    let err = String::from_utf8_lossy(&done.stderr).into_owned();
    Some(if done.status.success() { Ok(out) } else { Err(err.trim().lines().take(3).collect::<Vec<_>>().join(" ")) })
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// gh's checks in one word: failing when any failed, pending while any runs, else passing.
fn gh_checks(rollup: &Value) -> Option<String> {
    let items = rollup.as_array().filter(|items| !items.is_empty())?;
    let word = |item: &Value| {
        let conclusion =
            item["conclusion"].as_str().or_else(|| item["state"].as_str()).unwrap_or_default().to_ascii_uppercase();
        let status = item["status"].as_str().unwrap_or_default().to_ascii_uppercase();
        if ["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE"]
            .contains(&conclusion.as_str())
        {
            2
        } else if ["PENDING", "EXPECTED", "QUEUED", "IN_PROGRESS", "WAITING", "REQUESTED"]
            .contains(&conclusion.as_str())
            || ["QUEUED", "IN_PROGRESS", "WAITING", "PENDING", "REQUESTED"].contains(&status.as_str())
            || (conclusion.is_empty() && status != "COMPLETED")
        {
            1
        } else {
            0
        }
    };
    Some(match items.iter().map(word).max().unwrap_or(0) {
        2 => "failing".into(),
        1 => "pending".into(),
        _ => "passing".into(),
    })
}

/// `gh pr list --json number,title,headRefName,state,isDraft,url,statusCheckRollup,reviewDecision`.
pub fn parse_gh(json: &str) -> Result<Vec<HostPull>, String> {
    let items: Vec<Value> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(items
        .iter()
        .map(|item| {
            let state = text(item, "state").to_ascii_lowercase();
            HostPull {
                number: item["number"].as_u64().unwrap_or(0) as u32,
                title: text(item, "title"),
                branch: text(item, "headRefName"),
                state: if state == "open" && item["isDraft"].as_bool() == Some(true) { "draft".into() } else { state },
                checks: gh_checks(&item["statusCheckRollup"]),
                review: match text(item, "reviewDecision").as_str() {
                    "APPROVED" => Some("approved".into()),
                    "CHANGES_REQUESTED" => Some("changes requested".into()),
                    "REVIEW_REQUIRED" => Some("review required".into()),
                    _ => None,
                },
                url: text(item, "url"),
            }
        })
        .collect())
}

/// `glab mr list --output json`: GitLab's merge requests as its API gives them.
pub fn parse_glab(json: &str) -> Result<Vec<HostPull>, String> {
    let items: Vec<Value> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(items
        .iter()
        .map(|item| {
            let state = match text(item, "state").as_str() {
                "opened" => "open".to_owned(),
                other => other.to_owned(),
            };
            let draft = item["draft"].as_bool() == Some(true) || item["work_in_progress"].as_bool() == Some(true);
            // The list leaves the pipeline out (glab 1.124); what blocks merging says some of it.
            let merge = text(item, "detailed_merge_status");
            let pipeline = item["head_pipeline"]["status"].as_str().or_else(|| item["pipeline"]["status"].as_str());
            let checks = match pipeline {
                Some("success") => Some("passing".into()),
                Some("failed" | "canceled") => Some("failing".into()),
                Some(_) => Some("pending".into()),
                None => match merge.as_str() {
                    "ci_still_running" => Some("pending".into()),
                    "ci_must_pass" => Some("failing".into()),
                    _ => None,
                },
            };
            HostPull {
                number: item["iid"].as_u64().unwrap_or(0) as u32,
                title: text(item, "title"),
                branch: text(item, "source_branch"),
                state: if state == "open" && draft { "draft".into() } else { state },
                checks,
                review: match merge.as_str() {
                    "requested_changes" => Some("changes requested".into()),
                    "not_approved" => Some("review required".into()),
                    _ => None,
                },
                url: text(item, "web_url"),
            }
        })
        .collect())
}

/// `tea pulls list --output json --fields index,title,state,head,url`.
pub fn parse_tea(json: &str) -> Result<Vec<HostPull>, String> {
    let items: Vec<Value> = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(items
        .iter()
        .map(|item| {
            let number = item["index"].as_u64().or_else(|| text(item, "index").parse().ok()).unwrap_or(0) as u32;
            HostPull {
                number,
                title: text(item, "title"),
                branch: text(item, "head"),
                state: text(item, "state").to_ascii_lowercase(),
                checks: None,
                review: None,
                url: text(item, "url"),
            }
        })
        .collect())
}

/// The repository's path on its host: `https://github.com/owner/repo` is `owner/repo`.
fn path_of(page: &WebRemote) -> String {
    page.base
        .split_once("://")
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split_once('/'))
        .map(|(_, path)| path.to_owned())
        .unwrap_or_default()
}

/// The pull requests of the remote's repository, through its host's tool.
pub fn host_pulls(page: &WebRemote) -> HostPulls {
    let path = path_of(page);
    let host = page.host.clone();
    let nothing = |tool: Option<&str>, problem: Option<String>| HostPulls {
        tool: tool.map(str::to_owned),
        host: host.clone(),
        pulls: Vec::new(),
        problem,
    };
    let (tool, ran, parse): (&str, Option<Result<String, String>>, Parse) = match page.kind {
        WebKind::Github => {
            let repo = format!("{host}/{path}");
            let ran = run(
                "gh",
                &[
                    "pr",
                    "list",
                    "--repo",
                    &repo,
                    "--state",
                    "all",
                    "--limit",
                    LISTED,
                    "--json",
                    "number,title,headRefName,state,isDraft,url,statusCheckRollup,reviewDecision",
                ],
            );
            ("gh", ran, parse_gh)
        }
        WebKind::Gitlab => {
            let repo = format!("{host}/{path}");
            let ran = run("glab", &["mr", "list", "--repo", &repo, "--all", "--per-page", LISTED, "--output", "json"]);
            ("glab", ran, parse_glab)
        }
        WebKind::Gitea => {
            // tea picks the account by its login: the one for this host.
            let login = run("tea", &["logins", "list", "--output", "json"]).and_then(Result::ok).and_then(|json| {
                serde_json::from_str::<Vec<Value>>(&json)
                    .ok()?
                    .into_iter()
                    .find(|login| text(login, "url").contains(&host))
                    .map(|login| text(&login, "name"))
            });
            let mut args = vec![
                "pulls",
                "list",
                "--repo",
                path.as_str(),
                "--state",
                "all",
                "--limit",
                LISTED,
                "--output",
                "json",
                "--fields",
                "index,title,state,head,url",
            ];
            if let Some(login) = &login {
                args.extend(["--login", login.as_str()]);
            }
            let ran = run("tea", &args);
            ("tea", ran, parse_tea)
        }
        WebKind::Bitbucket => return nothing(None, None),
    };
    match ran {
        None => nothing(None, None),
        Some(Err(problem)) => nothing(Some(tool), Some(format!("{tool} could not list them: {problem}"))),
        Some(Ok(json)) => match parse(&json) {
            Ok(pulls) => HostPulls { tool: Some(tool.to_owned()), host, pulls, problem: None },
            Err(problem) => {
                nothing(Some(tool), Some(format!("{tool} answered in a way Haunt does not read: {problem}")))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gh_with_checks_drafts_and_reviews() {
        let json = r#"[
          {"number":12,"title":"Slugs","headRefName":"slugs","state":"OPEN","isDraft":false,"url":"https://github.com/o/r/pull/12",
           "statusCheckRollup":[{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SUCCESS"},{"__typename":"StatusContext","state":"FAILURE"}],
           "reviewDecision":"APPROVED"},
          {"number":11,"title":"WIP","headRefName":"wip","state":"OPEN","isDraft":true,"url":"u","statusCheckRollup":[{"status":"IN_PROGRESS","conclusion":""}],"reviewDecision":""},
          {"number":10,"title":"Old","headRefName":"old","state":"MERGED","isDraft":false,"url":"u","statusCheckRollup":[],"reviewDecision":"REVIEW_REQUIRED"}
        ]"#;
        let pulls = parse_gh(json).unwrap();
        let seen: Vec<String> = pulls
            .iter()
            .map(|pull| format!("#{} {} {} {:?} {:?}", pull.number, pull.branch, pull.state, pull.checks, pull.review))
            .collect();
        assert_eq!(
            seen,
            [
                r#"#12 slugs open Some("failing") Some("approved")"#,
                r#"#11 wip draft Some("pending") None"#,
                r#"#10 old merged None Some("review required")"#
            ]
        );
    }

    #[test]
    fn reads_glab_and_tea() {
        let glab = r#"[{"iid":3,"title":"Fix","source_branch":"fix","state":"opened","draft":false,"web_url":"https://gitlab.com/g/p/-/merge_requests/3","head_pipeline":{"status":"success"}},
                       {"iid":2,"title":"Done","source_branch":"done","state":"merged","draft":false,"web_url":"w","detailed_merge_status":"not_open"},
                       {"iid":4,"title":"Asked","source_branch":"asked","state":"opened","draft":false,"web_url":"w","detailed_merge_status":"requested_changes"},
                       {"iid":5,"title":"Running","source_branch":"running","state":"opened","draft":true,"web_url":"w","detailed_merge_status":"ci_still_running"}]"#;
        let pulls = parse_glab(glab).unwrap();
        assert_eq!(
            (pulls[0].number, pulls[0].state.as_str(), pulls[0].checks.as_deref()),
            (3, "open", Some("passing"))
        );
        assert_eq!((pulls[1].state.as_str(), pulls[1].checks.as_deref()), ("merged", None));
        // As glab 1.124 lists them: no pipeline, but the merge status.
        assert_eq!(pulls[2].review.as_deref(), Some("changes requested"));
        assert_eq!((pulls[3].state.as_str(), pulls[3].checks.as_deref()), ("draft", Some("pending")));
        let tea = r#"[{"index":"7","title":"Add","state":"open","head":"add","url":"https://gitea.example.com/o/r/pulls/7"}]"#;
        let pulls = parse_tea(tea).unwrap();
        assert_eq!((pulls[0].number, pulls[0].branch.as_str(), pulls[0].state.as_str()), (7, "add", "open"));
    }

    #[test]
    fn a_repository_path_from_its_page() {
        let page = crate::git::web_page("origin", "git@gitlab.com:group/sub/project.git").unwrap();
        assert_eq!(path_of(&page), "group/sub/project");
    }

    /// Asks gitlab.com for real through glab: run by hand, with glab installed and signed in
    /// (`cargo test asks_gitlab_through_glab -- --ignored`).
    #[test]
    #[ignore]
    fn asks_gitlab_through_glab() {
        let page = crate::git::web_page("origin", "https://gitlab.com/gitlab-org/cli.git").unwrap();
        let found = host_pulls(&page);
        assert_eq!((found.tool.as_deref(), found.problem.as_deref()), (Some("glab"), None));
        assert!(found.pulls.len() > 5);
        assert!(found
            .pulls
            .iter()
            .all(|pull| pull.number > 0 && !pull.branch.is_empty() && pull.url.contains("/-/merge_requests/")));
        assert!(
            found.pulls.iter().any(|pull| pull.state == "open")
                && found.pulls.iter().any(|pull| pull.state == "merged")
        );
        for pull in found.pulls.iter().take(8) {
            println!("!{} {} {} {:?} {:?}", pull.number, pull.state, pull.branch, pull.checks, pull.review);
        }
    }
}
