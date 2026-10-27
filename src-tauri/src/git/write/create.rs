//! New repositories: a clone of a URL, or an empty one (`git init`), each in a new folder the window then opens.
//!
//! - A clone runs as an untrusted folder's fetch does (`write/mod.rs`): no hook, no `ext::` transport, no
//!   submodules; your own global and system config stay. Its questions (passwords, host keys) go to the window, its
//!   progress to `progress`, and it can be stopped (`cancel` with the folder it goes into).
//! - The folder is new, or an empty one; a name is one folder's name, never a path.

use std::path::Path;

use super::network::said;
use super::remotes::remote_url;
use super::Writer;

/// A new folder's name from the window: one name, not `.` or `..`, nothing a path could make of it.
fn folder_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('-')
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name.chars().any(char::is_control);
    if bad {
        return Err(format!("\"{name}\" cannot be a folder's name."));
    }
    Ok(name)
}

/// Where a new repository goes: `parent` exists, and `name` in it does not (or is an empty folder).
fn new_folder(parent: &str, name: &str) -> Result<(String, String), String> {
    let parent = crate::paths::canonical(parent).ok_or("The folder to put it in is not there.")?;
    let name = folder_name(name)?;
    let target = Path::new(&parent).join(name);
    if target.exists() {
        let empty = target.is_dir() && std::fs::read_dir(&target).is_ok_and(|mut entries| entries.next().is_none());
        if !empty {
            return Err(format!("{} is there already.", target.to_string_lossy()));
        }
    }
    Ok((parent, name.to_owned()))
}

/// Clones `url` into `parent`/`name`; the new repository's folder.
pub fn clone(url: &str, parent: &str, name: &str, progress: &dyn Fn(&str)) -> Result<String, String> {
    let url = remote_url(url)?;
    let (parent, name) = new_folder(parent, name)?;
    let writer = Writer { tabless: true, ..Writer::new(&parent, false) };
    let ran = writer.network(
        &["clone", "--progress", "--no-recurse-submodules", "--end-of-options", url, &name],
        &["submodule.recurse=false".into()],
        &[],
        progress,
    )?;
    if ran.cancelled {
        return Err("The clone was stopped.".into());
    }
    if !ran.ok {
        return Err(said(&ran.stderr));
    }
    crate::paths::canonical(&Path::new(&parent).join(&name).to_string_lossy()).ok_or("The clone is not there.".into())
}

/// Makes an empty repository in `parent`/`name` (made when missing); its folder.
pub fn init(parent: &str, name: &str) -> Result<String, String> {
    let (parent, name) = new_folder(parent, name)?;
    let target = Path::new(&parent).join(&name);
    std::fs::create_dir_all(&target).map_err(|error| format!("The folder could not be made: {error}"))?;
    let root = crate::paths::canonical(&target.to_string_lossy()).ok_or("The folder could not be made.")?;
    Writer::new(&root, false).git(&["init"], None, &[], None)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_one_folder_name_only() {
        assert_eq!(folder_name(" app "), Ok("app"));
        for bad in ["", ".", "..", "-x", "a/b", r"a\b", "C:", "a?b"] {
            assert!(folder_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn makes_an_empty_repository_once() {
        let base = std::env::temp_dir().join(format!("haunt-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let parent = base.to_string_lossy().into_owned();
        let root = init(&parent, "fresh").unwrap();
        assert!(Path::new(&root).join(".git").is_dir());
        assert!(init(&parent, "fresh").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
