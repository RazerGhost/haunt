//! Running other programs from inside an AppImage. The AppImage's runtime puts its own libraries and paths first in
//! Haunt's environment (`LD_LIBRARY_PATH`, `PATH`, `XDG_DATA_DIRS`, `PYTHONHOME`, the GTK and GIO module paths, all
//! pointing into `$APPDIR`), and a program that inherits them loads the AppImage's libraries instead of its own: Node
//! crashes on the AppImage's older libffi, and so could Bun, Deno, a test runner or `gh`. So every program Haunt
//! starts (`crate::process::command`) gets the environment as it was before: each variable's entries inside `$APPDIR`
//! taken out, and a variable left with nothing removed. Outside an AppImage nothing changes.
//!
//! Haunt's own program is the exception: git and ssh start it to ask for a password (askpass.rs), and it needs the
//! AppImage's libraries to load. `own_program` gives a small script that starts it with them.

use std::ffi::OsString;
use std::sync::OnceLock;

/// What the AppImage's runtime added, as changes to make to a program's environment: (name, None) to remove a
/// variable, (name, Some(value)) to set it to its value without the AppImage's entries. Empty outside an AppImage.
fn changes() -> &'static [(OsString, Option<OsString>)] {
    static CHANGES: OnceLock<Vec<(OsString, Option<OsString>)>> = OnceLock::new();
    CHANGES.get_or_init(|| {
        let vars: Vec<(String, String)> = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        undone(&vars).into_iter().map(|(name, value)| (OsString::from(name), value.map(OsString::from))).collect()
    })
}

/// The variables the AppImage itself sets, which a program started from Haunt has no use for.
const ITS_OWN: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];

/// How to undo what the AppImage's runtime did to `vars`; nothing when they are not an AppImage's (no `APPDIR`, or no
/// `APPIMAGE`).
fn undone(vars: &[(String, String)]) -> Vec<(String, Option<String>)> {
    let get = |name: &str| vars.iter().find(|(known, _)| known == name).map(|(_, value)| value.as_str());
    let (Some(appdir), Some(_)) = (get("APPDIR").filter(|dir| !dir.is_empty()), get("APPIMAGE")) else {
        return Vec::new();
    };
    // `$APPDIR/usr/lib/` and `$APPDIR` alike; never a folder that only starts with the same letters.
    let root = appdir.trim_end_matches('/');
    let inside = |entry: &str| entry == root || entry.starts_with(&format!("{root}/"));
    let mut out = Vec::new();
    for (name, value) in vars {
        if ITS_OWN.contains(&name.as_str()) {
            out.push((name.clone(), None));
            continue;
        }
        if !value.contains(root) {
            continue;
        }
        let kept: Vec<&str> = value.split(':').filter(|entry| !entry.is_empty() && !inside(entry)).collect();
        out.push((name.clone(), (!kept.is_empty()).then(|| kept.join(":"))));
    }
    out
}

/// Puts back the environment from before the AppImage changed it, for a program that is not Haunt itself.
pub fn outside(command: &mut std::process::Command) {
    for (name, value) in changes() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
}

/// The program git and ssh start to ask for a password: Haunt itself, and in an AppImage a script that starts it with
/// the AppImage's libraries (the environment git hands on has them taken out). None when it can't be found.
pub fn own_program() -> Option<String> {
    let program = std::env::current_exe().ok()?.to_string_lossy().replace('\\', "/");
    if changes().is_empty() {
        return Some(program);
    }
    static SCRIPT: OnceLock<Option<String>> = OnceLock::new();
    SCRIPT.get_or_init(|| script(&program)).clone()
}

#[cfg(unix)]
fn script(program: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let quote = |text: &str| format!("'{}'", text.replace('\'', r"'\''"));
    let libraries = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let text = format!(
        "#!/bin/sh\n# Haunt's own program, with the AppImage's libraries it needs (appimage.rs).\nLD_LIBRARY_PATH={} exec {} \"$@\"\n",
        quote(&libraries),
        quote(program)
    );
    let path = std::env::temp_dir().join(format!("haunt-askpass-{}.sh", std::process::id()));
    std::fs::write(&path, text).ok()?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[cfg(not(unix))]
fn script(program: &str) -> Option<String> {
    Some(program.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
    }

    #[test]
    fn takes_out_what_the_appimage_put_in() {
        let found = undone(&vars(&[
            ("APPDIR", "/tmp/.mount_HauntX"),
            ("APPIMAGE", "/home/me/Haunt.AppImage"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_HauntX/usr/lib/:/tmp/.mount_HauntX/usr/lib/x86_64-linux-gnu/"),
            ("PATH", "/tmp/.mount_HauntX/usr/bin/:/usr/local/bin:/usr/bin"),
            ("XDG_DATA_DIRS", "/tmp/.mount_HauntX/usr/share/:/usr/local/share:/usr/share"),
            ("PYTHONHOME", "/tmp/.mount_HauntX/usr/"),
            ("HOME", "/home/me"),
            ("PROJECTS", "/tmp/.mount_HauntXtra/stays"),
        ]));
        let get = |name: &str| found.iter().find(|(known, _)| known == name).map(|(_, value)| value.clone());
        assert_eq!(get("LD_LIBRARY_PATH"), Some(None), "only the AppImage's: removed");
        assert_eq!(get("PATH"), Some(Some("/usr/local/bin:/usr/bin".into())));
        assert_eq!(get("XDG_DATA_DIRS"), Some(Some("/usr/local/share:/usr/share".into())));
        assert_eq!(get("PYTHONHOME"), Some(None));
        assert_eq!(get("APPDIR"), Some(None));
        assert_eq!(get("APPIMAGE"), Some(None));
        assert_eq!(get("HOME"), None, "untouched");
        assert_eq!(
            get("PROJECTS"),
            Some(Some("/tmp/.mount_HauntXtra/stays".into())),
            "a folder that only starts alike stays"
        );
    }

    #[test]
    fn changes_nothing_outside_an_appimage() {
        assert!(undone(&vars(&[("PATH", "/usr/bin"), ("LD_LIBRARY_PATH", "/opt/lib")])).is_empty());
        assert!(
            undone(&vars(&[("APPDIR", "/tmp/x"), ("PATH", "/tmp/x/bin")])).is_empty(),
            "APPDIR alone is not an AppImage"
        );
    }
}
