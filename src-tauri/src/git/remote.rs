//! The current branch's remote side: what a pull would bring in (the commits on its upstream since the two went apart),
//! and the remote's address on the web (GitHub, GitLab, Bitbucket, Gitea), for opening a commit, a file or a branch in
//! the browser. Read-only, through the safe runner.

use serde::Serialize;
use specta::Type;

use super::cli::{git, top};
use super::types::{CommitInfo, COMMIT_FORMAT};

/// What a pull of the current branch would bring in.
#[derive(Debug, Clone, Serialize, Type)]
pub struct Incoming {
    /// The branch it tracks, as `origin/main`.
    pub upstream: String,
    /// Where the two went apart: the commits after it on each side are yours and theirs.
    pub base: String,
    /// The upstream's tip.
    pub tip: String,
    /// The upstream's commits after `base`, newest first (at most 200).
    pub commits: Vec<CommitInfo>,
    /// Your commits after `base`, not on the upstream.
    pub ahead: u32,
}

/// The current branch's incoming commits; None when it tracks nothing or HEAD is detached.
pub fn incoming(root: &str) -> Result<Option<Incoming>, String> {
    let Ok(upstream) = git(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]) else {
        return Ok(None);
    };
    let upstream = upstream.trim().to_owned();
    let tip = git(root, &["rev-parse", "@{upstream}"])?.trim().to_owned();
    let base = git(root, &["merge-base", "HEAD", "@{upstream}"])?.trim().to_owned();
    let range = format!("{base}..{tip}");
    let out = git(root, &["log", "--max-count=200", &format!("--format={COMMIT_FORMAT}"), &range])?;
    let commits = CommitInfo::all(&out);
    let ahead = git(root, &["rev-list", "--count", &format!("{base}..HEAD")])?.trim().parse().unwrap_or(0);
    Ok(Some(Incoming { upstream, base, tip, commits, ahead }))
}

/// The current branch's upstream tip, if it tracks one.
pub fn upstream_tip(root: &str) -> Option<String> {
    git(root, &["rev-parse", "-q", "--verify", "@{upstream}"])
        .ok()
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
}

/// How many commits `to` has that `from` does not.
pub fn count_between(root: &str, from: &str, to: &str) -> u32 {
    git(root, &["rev-list", "--count", &format!("{from}..{to}")])
        .ok()
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0)
}

/// The kind of web host, for the shape of its links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WebKind {
    Github,
    Gitlab,
    Bitbucket,
    Gitea,
}

/// A remote's project page on the web.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct WebRemote {
    /// The remote it is for (`origin`).
    pub remote: String,
    /// `https://github.com/owner/repo`, without a trailing slash.
    pub base: String,
    pub kind: WebKind,
    /// The host's name, for "Open on github.com".
    pub host: String,
    /// The opened folder's place in the repository ("" at the top, "packages/app/" deeper in), for file links.
    pub prefix: String,
}

/// A remote's URL as its project page: `git@github.com:o/r.git`, `ssh://git@host:22/o/r.git` and `https://host/o/r.git`
/// all become `https://host/o/r`. Hosts that are not GitHub, GitLab, Bitbucket or Gitea by name count as GitHub-like.
pub fn web_page(remote: &str, url: &str) -> Option<WebRemote> {
    let url = url.trim();
    let (host, path) = if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let rest = rest.rsplit_once('@').map_or(rest, |(_, after)| after);
        rest.split_once('/')?
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.rsplit_once('@').map_or(rest, |(_, after)| after);
        let (host, path) = rest.split_once('/')?;
        (host.split(':').next()?, path)
    } else if let Some((user_host, path)) = url.split_once(':') {
        // scp-like: git@github.com:owner/repo.git (but not a Windows path such as C:\repo)
        if user_host.len() <= 1 || path.starts_with('\\') || path.starts_with("//") {
            return None;
        }
        (user_host.rsplit_once('@').map_or(user_host, |(_, host)| host), path)
    } else {
        return None;
    };
    let host = host.to_lowercase();
    let path = path.trim_end_matches('/').trim_end_matches(".git").trim_matches('/');
    if host.is_empty() || path.split('/').count() < 2 || path.contains("..") {
        return None;
    }
    let kind = if host.contains("gitlab") {
        WebKind::Gitlab
    } else if host.contains("bitbucket") {
        WebKind::Bitbucket
    } else if host.contains("gitea") || host.contains("codeberg") || host.contains("forgejo") {
        WebKind::Gitea
    } else {
        WebKind::Github
    };
    Some(WebRemote {
        remote: remote.to_owned(),
        base: format!("https://{host}/{path}"),
        kind,
        host,
        prefix: String::new(),
    })
}

/// The web page of the current branch's remote, else of `origin`, else of the only remote; None when none has one.
pub fn web_remote(root: &str) -> Option<WebRemote> {
    let branch = git(root, &["symbolic-ref", "--short", "-q", "HEAD"]).ok().map(|name| name.trim().to_owned());
    let tracked = branch.and_then(|branch| git(root, &["config", "--get", &format!("branch.{branch}.remote")]).ok());
    let remotes: Vec<String> = git(root, &["remote"])
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let name = tracked
        .map(|name| name.trim().to_owned())
        .filter(|name| remotes.contains(name))
        .or_else(|| remotes.iter().find(|name| *name == "origin").cloned())
        .or_else(|| (remotes.len() == 1).then(|| remotes[0].clone()))?;
    let url = git(root, &["remote", "get-url", "--", &name]).ok()?;
    let prefix = top(root).map(|(_, prefix)| prefix).unwrap_or_default();
    web_page(&name, &url).map(|page| WebRemote { prefix, ..page })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_remote_urls_into_project_pages() {
        let page = |url: &str| web_page("origin", url).map(|page| (page.base, page.kind));
        assert_eq!(
            page("git@github.com:razerghost/haunt.git"),
            Some(("https://github.com/razerghost/haunt".into(), WebKind::Github))
        );
        assert_eq!(
            page("https://github.com/razerghost/haunt"),
            Some(("https://github.com/razerghost/haunt".into(), WebKind::Github))
        );
        assert_eq!(
            page("https://user:token@gitlab.example.com/group/sub/app.git"),
            Some(("https://gitlab.example.com/group/sub/app".into(), WebKind::Gitlab))
        );
        assert_eq!(
            page("ssh://git@bitbucket.org:22/team/app.git"),
            Some(("https://bitbucket.org/team/app".into(), WebKind::Bitbucket))
        );
        assert_eq!(page("git@codeberg.org:me/app.git"), Some(("https://codeberg.org/me/app".into(), WebKind::Gitea)));
        // Paths on disk and nonsense have no page.
        assert_eq!(page("C:\\Users\\me\\repo.git"), None);
        assert_eq!(page("../remote.git"), None);
        assert_eq!(page("/srv/git/app.git"), None);
        assert_eq!(page("https://github.com/only-one"), None);
    }
}
