//! Pull request status per branch, through the host's own command-line tool: `gh` for GitHub, `glab` for GitLab,
//! `tea` for Gitea (untested: its output is read as tea documents it, never tried against a real Gitea), picked by the
//! remote's host. Only when switched on in Settings (off by default); without the tool,
//! or signed out, nothing changes but a note. The tool runs in a folder of its own with the repository named on its
//! command line, so nothing of the repository (its config, its hooks) runs; it uses the account you signed in with.
//! A new pull request is made the same way, when asked for (`create_pull`), whether or not the status is switched on.

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
    run_for(program, args, TIMEOUT)
}

fn run_for(program: &str, args: &[&str], timeout: Duration) -> Option<Result<String, String>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GLAB_NO_PROMPT", "1")
        .stdin(Stdio::null());
    let done = match crate::process::run(command, timeout) {
        Ok(done) => done,
        Err(crate::process::Failure::NotFound) => return None,
        Err(crate::process::Failure::TimedOut) => {
            return Some(Err(format!("{program} did not answer within {} seconds.", timeout.as_secs())))
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

/// A pull request to make, from the window.
#[derive(Debug, Clone, serde::Deserialize, Type)]
pub struct NewPull {
    /// The branch it is from, pushed already.
    pub branch: String,
    /// The branch it goes into.
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// The pull request made: its page.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CreatedPull {
    pub tool: String,
    pub url: Option<String>,
}

/// The tool and its arguments for `pull` on the remote's host; every value goes in one `--flag=value` argument, so
/// a title or a branch can never be read as an option.
fn create_args(page: &WebRemote, pull: &NewPull) -> Result<(&'static str, Vec<String>), String> {
    let path = path_of(page);
    let repo = format!("{}/{path}", page.host);
    let mut args: Vec<String> = match page.kind {
        WebKind::Github => vec![
            "pr".into(),
            "create".into(),
            format!("--repo={repo}"),
            format!("--head={}", pull.branch),
            format!("--base={}", pull.base),
            format!("--title={}", pull.title),
            format!("--body={}", pull.body),
        ],
        WebKind::Gitlab => vec![
            "mr".into(),
            "create".into(),
            format!("--repo={repo}"),
            format!("--source-branch={}", pull.branch),
            format!("--target-branch={}", pull.base),
            format!("--title={}", pull.title),
            format!("--description={}", pull.body),
            "--yes".into(),
        ],
        WebKind::Gitea => vec![
            "pulls".into(),
            "create".into(),
            format!("--repo={path}"),
            format!("--head={}", pull.branch),
            format!("--base={}", pull.base),
            format!("--title={}", pull.title),
            format!("--description={}", pull.body),
        ],
        WebKind::Bitbucket => return Err("Haunt has no tool to make pull requests on Bitbucket.".into()),
    };
    if pull.draft {
        match page.kind {
            WebKind::Github | WebKind::Gitlab => args.push("--draft".into()),
            _ => return Err("Gitea's tea does not make drafts.".into()),
        }
    }
    let tool = match page.kind {
        WebKind::Github => "gh",
        WebKind::Gitlab => "glab",
        _ => "tea",
    };
    Ok((tool, args))
}

/// The last web address in a tool's answer: the new pull request's page.
fn created_url(said: &str) -> Option<String> {
    said.split_whitespace()
        .rfind(|word| word.starts_with("https://"))
        .map(|url| url.trim_end_matches(['.', ',']).to_owned())
}

/// Makes a pull request (a merge request on GitLab) on the remote's host, through its tool.
pub fn create_pull(page: &WebRemote, pull: &NewPull) -> Result<CreatedPull, String> {
    let names_ok = |name: &str| !name.is_empty() && !name.starts_with('-') && !name.contains(char::is_whitespace);
    if !names_ok(&pull.branch) || !names_ok(&pull.base) {
        return Err("Pick the branch it is from and the one it goes into.".into());
    }
    if pull.title.trim().is_empty() {
        return Err("A pull request needs a title.".into());
    }
    let (tool, args) = create_args(page, pull)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run_for(tool, &args, Duration::from_secs(60)) {
        None => Err(format!(
            "{tool} is not installed. Install it and sign in ({tool} auth login) to make pull requests from Haunt."
        )),
        Some(Err(problem)) => Err(format!("{tool} could not make it: {problem}")),
        Some(Ok(said)) => Ok(CreatedPull { tool: tool.to_owned(), url: created_url(&said) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_pull_requests_with_every_value_in_its_own_argument() {
        let page = WebRemote {
            remote: "origin".into(),
            base: "https://github.com/o/r".into(),
            kind: WebKind::Github,
            host: "github.com".into(),
            prefix: String::new(),
        };
        let pull = NewPull {
            branch: "feature".into(),
            base: "main".into(),
            title: "--delete everything".into(),
            body: "Why

- a"
            .into(),
            draft: true,
        };
        let (tool, args) = create_args(&page, &pull).unwrap();
        assert_eq!(tool, "gh");
        assert_eq!(
            args,
            [
                "pr",
                "create",
                "--repo=github.com/o/r",
                "--head=feature",
                "--base=main",
                "--title=--delete everything",
                "--body=Why

- a",
                "--draft"
            ]
        );
        let lab = WebRemote {
            base: "https://gitlab.com/g/r".into(),
            kind: WebKind::Gitlab,
            host: "gitlab.com".into(),
            ..page.clone()
        };
        let (tool, args) = create_args(&lab, &NewPull { draft: false, ..pull.clone() }).unwrap();
        assert_eq!(tool, "glab");
        assert!(args.contains(&"--source-branch=feature".to_owned()) && args.contains(&"--yes".to_owned()));
        assert!(create_pull(&page, &NewPull { branch: "-x".into(), ..pull.clone() }).is_err());
        assert!(create_pull(&page, &NewPull { title: " ".into(), ..pull }).is_err());
    }

    #[test]
    fn finds_the_new_pull_requests_page_in_the_answer() {
        assert_eq!(
            created_url(
                "https://github.com/o/r/pull/12
"
            )
            .as_deref(),
            Some("https://github.com/o/r/pull/12")
        );
        assert_eq!(
            created_url(
                "Creating merge request for feature into main in g/r

!7 Title (feature)
 https://gitlab.com/g/r/-/merge_requests/7
"
            )
            .as_deref(),
            Some("https://gitlab.com/g/r/-/merge_requests/7")
        );
        assert_eq!(created_url("done"), None);
    }

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
