//! A commit's signature, checked: who signed it and whether the signature holds, as git sees it (`%G?`).
//!
//! Checking runs a program (gpg, gpgsm or ssh-keygen), and which one is a git setting a repository's own config could
//! name: a repository you have not trusted could make that any command, or point SSH checking at its own list of
//! allowed signers and pass its commits off as yours. So only your own settings count here: the programs and the
//! SSH signer files come from your global (or system) config, else git's defaults, whatever the repository says.

use serde::Serialize;
use specta::Type;

use super::cli::{git, revision};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatus {
    /// Good, and the key is one you trust.
    Good,
    /// Good, but the key's trust is not known (a gpg key you never marked, an SSH key not in your allowed signers).
    Unknown,
    /// Good, made by a key that has expired since, or a signature that has itself expired.
    Expired,
    /// Good, made by a key revoked since.
    Revoked,
    /// It does not hold: the commit changed after it was signed, or it was not signed by that key.
    Bad,
    /// It could not be checked: the key is missing, or the program is.
    Unchecked,
    /// Not signed.
    None,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct Signature {
    pub status: SignatureStatus,
    /// gpg, ssh or x509, from how the signature is written.
    pub format: Option<String>,
    /// Who the signature says signed it (a name and email for gpg, a principal for SSH); empty when not known.
    pub signer: String,
    /// The key, as the program names it.
    pub key: String,
}

fn status(code: &str) -> SignatureStatus {
    match code.trim() {
        "G" => SignatureStatus::Good,
        "U" => SignatureStatus::Unknown,
        "X" | "Y" => SignatureStatus::Expired,
        "R" => SignatureStatus::Revoked,
        "B" => SignatureStatus::Bad,
        "E" => SignatureStatus::Unchecked,
        _ => SignatureStatus::None,
    }
}

/// The settings that pick what checking runs and which signers count, from your global or system config only.
fn yours(root: &str) -> Vec<String> {
    let mine = |key: &str| {
        ["--global", "--system"].iter().find_map(|scope| {
            git(root, &["config", scope, "--get", key])
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
    };
    let mut out = Vec::new();
    for (key, fallback) in [
        ("gpg.program", "gpg"),
        ("gpg.openpgp.program", "gpg"),
        ("gpg.x509.program", "gpgsm"),
        ("gpg.ssh.program", "ssh-keygen"),
    ] {
        out.push("-c".to_owned());
        out.push(format!("{key}={}", mine(key).unwrap_or_else(|| fallback.to_owned())));
    }
    for key in ["gpg.ssh.allowedSignersFile", "gpg.ssh.revocationFile"] {
        out.push("-c".to_owned());
        out.push(format!("{key}={}", mine(key).unwrap_or_default()));
    }
    out
}

/// The signature's format, from its first line in the commit.
fn format_of(root: &str, sha: &str) -> Option<String> {
    let raw = git(root, &["cat-file", "commit", sha]).ok()?;
    let header = raw.split("\n\n").next()?;
    let start =
        header.lines().find_map(|line| line.strip_prefix("gpgsig ").or_else(|| line.strip_prefix("gpgsig-sha256 ")))?;
    Some(
        if start.contains("SSH SIGNATURE") {
            "ssh"
        } else if start.contains("BEGIN SIGNED MESSAGE") {
            "x509"
        } else {
            "gpg"
        }
        .to_owned(),
    )
}

pub fn check(root: &str, sha: &str) -> Result<Signature, String> {
    let sha = revision(sha)?;
    let format = format_of(root, sha);
    if format.is_none() {
        return Ok(Signature { status: SignatureStatus::None, format, signer: String::new(), key: String::new() });
    }
    let mut args = yours(root);
    args.extend(["log", "-1", "--no-walk", "--format=%G?%x1f%GS%x1f%GK", "--end-of-options", sha].map(str::to_owned));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = git(root, &args)?;
    let mut parts = out.trim_end_matches(['\r', '\n']).split('\x1f');
    let code = parts.next().unwrap_or_default();
    let signer = parts.next().unwrap_or_default().trim().to_owned();
    let key = parts.next().unwrap_or_default().trim().to_owned();
    // A signature git found but could not read at all (no program for it) says N; it is there, just unchecked.
    let status = match status(code) {
        SignatureStatus::None => SignatureStatus::Unchecked,
        found => found,
    };
    Ok(Signature { status, format, signer, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gits_word_for_a_signature() {
        assert_eq!(status("G"), SignatureStatus::Good);
        assert_eq!(status("U\n"), SignatureStatus::Unknown);
        assert_eq!(status("Y"), SignatureStatus::Expired);
        assert_eq!(status("B"), SignatureStatus::Bad);
        assert_eq!(status("E"), SignatureStatus::Unchecked);
        assert_eq!(status("N"), SignatureStatus::None);
    }

    #[test]
    fn an_unsigned_commit_is_not_checked_and_the_repositorys_programs_never_run() {
        let r = crate::git::tests::Repo::new();
        r.sh(&["commit", "-q", "--allow-empty", "-m", "plain"]);
        let sha = r.sh(&["rev-parse", "HEAD"]).trim().to_owned();
        assert_eq!(check(&r.root, &sha).unwrap().status, SignatureStatus::None);

        // A commit with a made-up SSH signature, in a repository whose config names its own program and signers.
        let marker = r.path("ran");
        let script = r.path("fake-signer");
        let touch = format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.to_string_lossy().replace('\\', "/"));
        std::fs::write(&script, touch).unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let script = script.to_string_lossy().replace('\\', "/");
        r.sh(&["config", "gpg.ssh.program", &script]);
        r.sh(&["config", "gpg.program", &script]);
        r.sh(&["config", "gpg.ssh.allowedSignersFile", "signers"]);
        let tree = r.sh(&["rev-parse", "HEAD^{tree}"]).trim().to_owned();
        let body = format!(
            "tree {tree}\nauthor T <t@t> 1 +0000\ncommitter T <t@t> 1 +0000\ngpgsig -----BEGIN SSH SIGNATURE-----\n U1NIU0lH\n -----END SSH SIGNATURE-----\n\nsigned\n"
        );
        std::fs::write(r.path("commit.txt"), body).unwrap();
        let signed = r.sh(&["hash-object", "-t", "commit", "-w", "commit.txt"]).trim().to_owned();
        // Plain git does run the repository's program: what Haunt must not do.
        let _ = std::process::Command::new("git")
            .args(["log", "-1", "--format=%G?", &signed])
            .current_dir(&r.root)
            .output();
        assert!(marker.exists(), "the test's fake program should run under plain git");
        std::fs::remove_file(&marker).unwrap();
        let found = check(&r.root, &signed).unwrap();
        assert_eq!(found.format.as_deref(), Some("ssh"));
        assert_ne!(found.status, SignatureStatus::Good);
        assert!(!marker.exists(), "the repository's own program ran");
        assert_eq!(crate::git::objects::signed(&r.root, [sha.as_str(), signed.as_str()].into_iter()), [false, true]);
        assert!(check(&r.root, "--all").is_err());
    }
}
