//! A theme file for those who make their colours with a tool (matugen, pywal, wallust from the wallpaper): `theme.json`
//! in `haunt` in the system's config folder (`~/.config/haunt/theme.json` on Linux). What it sets wins over the
//! profile's own choices, in every profile; Appearance marks those fields. The folder is watched and a change applies
//! at once (the news `themeFile`); a file that does not read keeps the last good one, and says why once.
//! docs/theming.md describes the fields.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::events::Events;
use crate::settings::{self, hex, Appearance};

/// The colours of the window's own parts, each a hex: they win over the shades made from the main colour and greys.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Roles {
    /// The page.
    pub background: String,
    /// Cards, menus, the title bar.
    pub raised: String,
    pub text: String,
    /// Less important text.
    pub muted: String,
    pub border: String,
    /// The main colour exactly (buttons, links, what is chosen).
    pub primary: String,
    /// Text on the main colour.
    pub on_primary: String,
}

/// What the file sets; each empty field (or one it leaves out) is the profile's.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeFile {
    /// The fields Appearance has; `primary` and `neutral` are for both light and dark unless `darkPrimary` or
    /// `darkNeutral` say otherwise.
    #[serde(flatten)]
    pub look: Appearance,
    pub roles: Roles,
    /// The window's background from 0 (clear) to 1 (solid), for the compositor's blur behind it (Linux, read at start).
    pub opacity: Option<f64>,
}

/// The file as the window gets it.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFileState {
    pub path: String,
    /// What applies: the file's last good reading; null when there is no file.
    pub theme: Option<ThemeFile>,
    /// Why the file does not read now (the last good reading still applies).
    pub problem: Option<String>,
    /// The fields of a file that reads that were left out, by name (`primary`, `roles.text`).
    pub left_out: Vec<String>,
}

static STATE: Mutex<Option<ThemeFileState>> = Mutex::new(None);

/// `<config>/haunt/theme.json`.
pub fn path(config_root: &Path) -> PathBuf {
    config_root.join("haunt").join("theme.json")
}

/// Whether the window is made transparent, so the file's opacity can let the compositor's blur through: on Linux, when
/// the file read at start has an opacity below 1. Decided once; a later opacity applies after a restart.
pub fn transparent_window() -> bool {
    static AT_START: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AT_START.get_or_init(|| {
        cfg!(target_os = "linux")
            && current().theme.and_then(|theme| theme.opacity).is_some_and(|opacity| opacity < 1.0)
    })
}

pub fn current() -> ThemeFileState {
    STATE.lock().clone().unwrap_or_default()
}

/// A font name the window can put in CSS: letters, digits, spaces and a few marks, at most 64.
fn font_ok(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_alphanumeric() || " -_.".contains(c))
}

/// The file's text read: what applies, and what was left out (by name). Err when it is not a theme file at all.
fn parse(text: &str) -> Result<(ThemeFile, Vec<String>), String> {
    let mut file: ThemeFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut left_out = Vec::new();
    // Any font the system has, not only the bundled list; checked apart from the rest.
    let font = std::mem::take(&mut file.look.font);
    // The same colour for light and dark, unless the file says otherwise.
    if file.look.dark_primary.is_empty() {
        file.look.dark_primary = file.look.primary.clone();
    }
    if file.look.dark_neutral.is_empty() {
        file.look.dark_neutral = file.look.neutral.clone();
    }
    let given = file.look.clone();
    let checked = settings::checked(given.clone());
    for (name, before, after) in settings::fields(&given, &checked) {
        // Said once for a colour the file gave for both light and dark.
        let copied =
            (name == "darkPrimary" && given.primary == before) || (name == "darkNeutral" && given.neutral == before);
        if !before.is_empty() && after.is_empty() && !copied {
            left_out.push(name);
        }
    }
    file.look = checked;
    // An empty theme is the profile's, not Haunt's own.
    if given.theme.is_empty() {
        file.look.theme.clear();
    }
    if !font.is_empty() {
        if font_ok(&font) {
            file.look.font = font;
        } else {
            left_out.push("font".into());
        }
    }
    let roles = &mut file.roles;
    for (name, value) in [
        ("roles.background", &mut roles.background),
        ("roles.raised", &mut roles.raised),
        ("roles.text", &mut roles.text),
        ("roles.muted", &mut roles.muted),
        ("roles.border", &mut roles.border),
        ("roles.primary", &mut roles.primary),
        ("roles.onPrimary", &mut roles.on_primary),
    ] {
        if value.is_empty() {
            continue;
        }
        match hex(value) {
            Some(color) => *value = color,
            None => {
                left_out.push(name.into());
                value.clear();
            }
        }
    }
    if let Some(opacity) = file.opacity {
        if !(0.0..=1.0).contains(&opacity) {
            left_out.push("opacity".into());
            file.opacity = None;
        }
    }
    Ok((file, left_out))
}

/// Reads the file again; the news `themeFile` when what applies or the problem changed.
fn read(file: &Path, events: Option<&Events>) {
    let shown = file.to_string_lossy().into_owned();
    let mut state = STATE.lock();
    let before = state.clone();
    let last_good = before.as_ref().and_then(|known| known.theme.clone());
    let next = match std::fs::read_to_string(file) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ThemeFileState { path: shown, ..ThemeFileState::default() }
        }
        Err(error) => {
            ThemeFileState { path: shown, theme: last_good, problem: Some(error.to_string()), left_out: Vec::new() }
        }
        Ok(text) => match parse(&text) {
            Ok((theme, left_out)) => ThemeFileState { path: shown, theme: Some(theme), problem: None, left_out },
            Err(problem) => {
                ThemeFileState { path: shown, theme: last_good, problem: Some(problem), left_out: Vec::new() }
            }
        },
    };
    if before.as_ref() == Some(&next) {
        return;
    }
    *state = Some(next.clone());
    drop(state);
    if let Some(events) = events {
        events.send(None, serde_json::json!({ "type": "themeFile", "state": next }));
    }
}

/// Reads the file now, and again whenever it changes (its folder is watched, and a linked file's too).
pub fn start(config_root: &Path, events: Arc<Events>) {
    let file = path(config_root);
    read(&file, None);
    let Some(folder) = file.parent().map(Path::to_path_buf) else { return };
    // The folder is made so it can be watched before the file exists.
    let _ = std::fs::create_dir_all(&folder);
    let mut watched = vec![file.clone()];
    // A link to a file elsewhere (pywal writes in its own cache): that file is watched too.
    if let Ok(target) = std::fs::canonicalize(&file) {
        if target.parent() != std::fs::canonicalize(&folder).ok().as_deref() {
            watched.push(target);
        }
    }
    for path in watched {
        let (file, events) = (file.clone(), events.clone());
        // Kept for as long as Haunt runs.
        std::mem::forget(crate::watch::file(&path, move || read(&file, Some(&events))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_a_wallpaper_tool_writes() {
        let (file, left_out) = parse(
            r##"{
                "mode": "dark",
                "primary": "#D0BCFF",
                "neutral": "#141218",
                "error": "#f2b8b5",
                "font": "Inter",
                "opacity": 0.85,
                "roles": { "background": "#141218", "raised": "#211f26", "text": "#e6e0e9", "muted": "#cac4d0",
                           "border": "#49454f", "primary": "#d0bcff", "onPrimary": "#381e72" }
            }"##,
        )
        .unwrap();
        assert!(left_out.is_empty(), "{left_out:?}");
        assert_eq!(file.look.mode, "dark");
        assert_eq!(file.look.primary, "#d0bcff");
        assert_eq!(file.look.dark_primary, "#d0bcff", "for both light and dark");
        assert_eq!(file.look.dark_neutral, "#141218");
        assert_eq!(file.look.theme, "", "the profile's theme");
        assert_eq!(file.look.font, "Inter");
        assert_eq!(file.roles.on_primary, "#381e72");
        assert_eq!(file.opacity, Some(0.85));
    }

    #[test]
    fn leaves_out_what_does_not_read_and_says_so() {
        let (file, left_out) = parse(
            r##"{ "mode": "dusk", "primary": "chartreuse", "neutral": "zinc", "font": "Inter\"; }",
                  "roles": { "text": "white" }, "opacity": 2, "comment": "made by matugen" }"##,
        )
        .unwrap();
        assert_eq!(file.look.neutral, "zinc");
        assert_eq!(file.look.mode, "");
        assert_eq!(file.roles.text, "");
        assert_eq!(file.opacity, None);
        for name in ["mode", "primary", "font", "roles.text", "opacity"] {
            assert!(left_out.iter().any(|known| known == name), "{name} in {left_out:?}");
        }
        assert!(parse("not json").is_err());
        assert!(parse(r##"{ "roles": { "shadow": "#000000" } }"##).is_err(), "a role Haunt does not know is a mistake");
    }

    #[test]
    fn keeps_the_last_good_file_while_a_new_one_does_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        read(&file, None);
        assert_eq!(current().theme, None);
        std::fs::write(&file, r##"{ "primary": "#3584e4" }"##).unwrap();
        read(&file, None);
        assert_eq!(current().theme.unwrap().look.primary, "#3584e4");
        std::fs::write(&file, "{ half").unwrap();
        read(&file, None);
        let state = current();
        assert_eq!(state.theme.unwrap().look.primary, "#3584e4");
        assert!(state.problem.is_some());
        std::fs::remove_file(&file).unwrap();
        read(&file, None);
        assert_eq!(current().theme, None, "no file, nothing set");
    }
}
