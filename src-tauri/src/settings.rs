//! What Haunt keeps between launches: the open tabs, the one shown, the recent repositories, the trusted folders, and
//! the git settings Rust acts on (what a pull does when the branches went apart, how often to fetch in the background).
//! One JSON file in the app's config folder; a broken or missing file means a fresh start, never a failure.
//!
//! Profiles split the repositories into groups (work, your own, a client's): each has its own tabs, the one shown and
//! its recent repositories. The profile in use keeps them in `root`, `tabs` and `recent`, the others in `profiles`.
//! Each profile has its own look too (`Appearance`: light or dark, Nuxt UI's main and neutral colours for each of the
//! two, the corners).
//! Trusted folders and the other settings are the same in every profile: trust is about a folder's code.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::paths::canonical;

const RECENT: usize = 6;
/// The profile's name when none was ever made.
pub const FIRST_PROFILE: &str = "Default";
/// How often a background fetch may run, in minutes; 0 is off.
pub const FETCH_EVERY: [u32; 4] = [0, 5, 15, 30];

/// What a pull does when your branch and its remote branch both have commits the other lacks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PullChoice {
    /// Ask each time.
    #[default]
    Ask,
    Rebase,
    Merge,
}

/// What a look may choose, the same lists as the window's (shared/themes.ts): the themes, Nuxt UI's colours (any
/// colour for any role, "neutral" as the main colour being plain black or white), its greys, the corner radii (rem),
/// the root font sizes (px), the fonts bundled into the window ("system" is the system's own), the icon sets, the
/// default variants per group of components, the sizes, and light or dark.
pub const THEMES: [&str; 13] = [
    "haunt",
    "default",
    "mono",
    "cobalt",
    "sky",
    "mint",
    "iris",
    "crimson",
    "coral",
    "sunset",
    "carbon",
    "bubblegum",
    "parchment",
];
pub const PRIMARY: [&str; 18] = [
    "neutral", "red", "orange", "amber", "yellow", "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo",
    "violet", "purple", "fuchsia", "pink", "rose",
];
/// The main colour that is the desktop's accent (Linux), or the theme's where the desktop has none.
pub const SYSTEM_ACCENT: &str = "system";
pub const NEUTRAL: [&str; 9] = ["slate", "gray", "zinc", "neutral", "stone", "taupe", "mauve", "mist", "olive"];
pub const RADIUS: [&str; 8] = ["0", "0.125", "0.25", "0.375", "0.5", "0.625", "0.75", "1"];
pub const FONT_SIZE: [&str; 5] = ["14", "15", "16", "17", "18"];
pub const FONTS: [&str; 15] = [
    "system",
    "Public Sans",
    "Geist",
    "Geist Mono",
    "Inter",
    "DM Sans",
    "Roboto",
    "Figtree",
    "Nunito",
    "Manrope",
    "Plus Jakarta Sans",
    "Bricolage Grotesque",
    "Outfit",
    "Poppins",
    "Source Serif 4",
];
pub const ICONS: [&str; 8] = ["lucide", "bootstrap", "phosphor", "tabler", "remix", "material", "heroicons", "iconoir"];
pub const BUTTONS: [&str; 6] = ["solid", "outline", "soft", "subtle", "ghost", "link"];
pub const PANELS: [&str; 4] = ["solid", "outline", "soft", "subtle"];
pub const INPUTS: [&str; 5] = ["outline", "soft", "subtle", "ghost", "none"];
pub const SIZES: [&str; 5] = ["xs", "sm", "md", "lg", "xl"];
pub const MODE: [&str; 3] = ["system", "light", "dark"];

/// How a profile looks: a theme (shared/themes.ts), and the profile's own choices on top of it, each empty one the
/// theme's: the main colour and greys while the window is light (`primary`, `neutral`) and while it is dark
/// (`dark_primary`, `dark_neutral`), the other colour roles, the corners, the font and its size, the icons, and the
/// default variants and size of the components. `mode` is empty until the window first says which it uses.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub mode: String,
    pub theme: String,
    pub primary: String,
    pub neutral: String,
    pub dark_primary: String,
    pub dark_neutral: String,
    pub secondary: String,
    pub success: String,
    pub info: String,
    pub warning: String,
    pub error: String,
    pub radius: String,
    pub font: String,
    pub font_size: String,
    pub icons: String,
    pub buttons: String,
    pub inputs: String,
    pub panels: String,
    pub size: String,
}

impl Appearance {
    /// The same, with anything unknown left to the theme (empty), and Haunt's own theme where none is named.
    fn checked(self) -> Appearance {
        let pick = |value: String, known: &[&str]| if known.contains(&value.as_str()) { value } else { String::new() };
        // A named colour, or any colour as a hex (shared/ramp.ts makes its shades).
        let named_or_hex = |value: String, known: &[&str]| hex(&value).unwrap_or_else(|| pick(value, known));
        let color = |value: String| named_or_hex(value, &[&PRIMARY[..], &NEUTRAL[..]].concat());
        // The main colour may also be the desktop's accent, whatever it is when shown (desktop.rs).
        let main = [&PRIMARY[..], &[SYSTEM_ACCENT]].concat();
        let theme = pick(self.theme, &THEMES);
        Appearance {
            mode: pick(self.mode, &MODE),
            theme: if theme.is_empty() { THEMES[0].to_owned() } else { theme },
            primary: named_or_hex(self.primary, &main),
            neutral: named_or_hex(self.neutral, &NEUTRAL),
            dark_primary: named_or_hex(self.dark_primary, &main),
            dark_neutral: named_or_hex(self.dark_neutral, &NEUTRAL),
            secondary: color(self.secondary),
            success: color(self.success),
            info: color(self.info),
            warning: color(self.warning),
            error: color(self.error),
            radius: pick(self.radius, &RADIUS),
            font: pick(self.font, &FONTS),
            font_size: pick(self.font_size, &FONT_SIZE),
            icons: pick(self.icons, &ICONS),
            buttons: pick(self.buttons, &BUTTONS),
            inputs: pick(self.inputs, &INPUTS),
            panels: pick(self.panels, &PANELS),
            size: pick(self.size, &SIZES),
        }
    }
}

/// A look with anything unknown left to the theme (`Appearance::checked`), for the theme file (theme_file.rs).
pub fn checked(appearance: Appearance) -> Appearance {
    appearance.checked()
}

/// Each field of two looks by its name as the window knows it (`darkPrimary`): (name, first's value, second's).
pub fn fields(first: &Appearance, second: &Appearance) -> Vec<(String, String, String)> {
    let as_map = |look: &Appearance| match serde_json::to_value(look) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let (first, second) = (as_map(first), as_map(second));
    let text =
        |value: Option<&serde_json::Value>| value.and_then(|value| value.as_str()).unwrap_or_default().to_owned();
    first.keys().map(|name| (name.clone(), text(first.get(name)), text(second.get(name)))).collect()
}

/// A colour as `#rrggbb` in lower case, from `#rrggbb` or `#rgb`; None when it is not a hex.
pub fn hex(value: &str) -> Option<String> {
    let digits = value.trim().strip_prefix('#')?;
    if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let digits = match digits.len() {
        6 => digits.to_owned(),
        3 => digits.chars().flat_map(|digit| [digit, digit]).collect(),
        _ => return None,
    };
    Some(format!("#{}", digits.to_ascii_lowercase()))
}

/// The colours a tab may have (Tailwind's names, which the window draws).
pub const TAB_COLORS: [&str; 8] = ["red", "orange", "amber", "green", "teal", "blue", "violet", "pink"];

/// How a folder's tab looks, in every profile: pinned (first in the row, its name hidden, no close button), and a
/// colour ("" for none).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct TabLook {
    pub pinned: bool,
    pub color: String,
}

/// A profile not in use: its tabs, the one shown and its recent repositories, kept until it is switched to.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub root: Option<String>,
    pub tabs: Vec<String>,
    pub recent: Vec<String>,
    pub appearance: Appearance,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The repository shown.
    pub root: Option<String>,
    /// The repositories open as tabs, in order.
    pub tabs: Vec<String>,
    pub recent: Vec<String>,
    /// Folders whose own code may run (their TypeScript, an architecture script).
    pub trusted: Vec<String>,
    pub pull: PullChoice,
    /// Minutes between background fetches of the repository shown; 0 is off.
    pub fetch_every: u32,
    /// The profile in use ("" before any was made: `FIRST_PROFILE`).
    pub profile: String,
    /// The profiles not in use.
    pub profiles: Vec<Profile>,
    /// How the profile in use looks.
    pub appearance: Appearance,
    /// Pull request status from the host's own tool (gh, glab, tea), when installed and signed in. Off by default.
    pub host_tools: bool,
    /// Whether Haunt looks for a new version of itself now and then (GitHub's releases). Off by default.
    pub update_check: bool,
    /// The system's own window frame and title bar instead of Haunt's buttons (Windows and Linux). Off by default.
    pub system_title_bar: bool,
    /// Pinned and coloured tabs, by folder.
    pub tab_looks: HashMap<String, TabLook>,
    /// The folders the repository manager looks through for repositories, for every profile.
    pub repository_folders: Vec<String>,
}

impl Settings {
    /// Reads the file; every path is put in its one form, and folders that no longer exist are dropped.
    pub fn load(file: &Path) -> Settings {
        let read: Settings =
            fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        let clean = |paths: Vec<String>| {
            let mut out: Vec<String> = Vec::new();
            for path in paths.iter().filter_map(|path| canonical(path)) {
                if !out.contains(&path) {
                    out.push(path);
                }
            }
            out
        };
        let root = read.root.as_deref().and_then(canonical);
        // Settings from before the tabs: the one repository is the one tab.
        let mut tabs = clean(read.tabs);
        if let Some(root) = &root {
            if !tabs.contains(root) {
                tabs.push(root.clone());
            }
        }
        // Trusted folders are kept even while they are missing (an unplugged drive), in their one form where possible.
        let trusted = read.trusted.iter().map(|path| canonical(path).unwrap_or_else(|| path.clone())).collect();
        // The same for the repository folders.
        let mut repository_folders: Vec<String> = Vec::new();
        for path in read.repository_folders.iter().map(|path| canonical(path).unwrap_or_else(|| path.clone())) {
            if !repository_folders.contains(&path) {
                repository_folders.push(path);
            }
        }
        let fetch_every = if FETCH_EVERY.contains(&read.fetch_every) { read.fetch_every } else { 0 };
        let profile = if read.profile.trim().is_empty() { FIRST_PROFILE.to_owned() } else { read.profile };
        let mut profiles: Vec<Profile> = Vec::new();
        for other in read.profiles {
            let name = other.name.trim().to_owned();
            if name.is_empty() || name == profile || profiles.iter().any(|known| known.name == name) {
                continue;
            }
            let root = other.root.as_deref().and_then(canonical);
            let appearance = other.appearance.checked();
            profiles.push(Profile { name, root, tabs: clean(other.tabs), recent: clean(other.recent), appearance });
        }
        let appearance = read.appearance.checked();
        let tab_looks = read
            .tab_looks
            .into_iter()
            .filter_map(|(path, look)| {
                let color = if TAB_COLORS.contains(&look.color.as_str()) { look.color } else { String::new() };
                let look = TabLook { pinned: look.pinned, color };
                (look != TabLook::default()).then(|| canonical(&path).map(|path| (path, look))).flatten()
            })
            .collect();
        let mut settings = Settings {
            root,
            tabs,
            recent: clean(read.recent),
            trusted,
            pull: read.pull,
            fetch_every,
            profile,
            profiles,
            appearance,
            host_tools: read.host_tools,
            update_check: read.update_check,
            system_title_bar: read.system_title_bar,
            tab_looks,
            repository_folders,
        };
        settings.pinned_first();
        settings
    }

    /// Changes how a tab looks; a pinned tab goes to the end of the pinned ones.
    pub fn set_tab_look(&mut self, root: &str, look: TabLook) -> Result<(), String> {
        if !look.color.is_empty() && !TAB_COLORS.contains(&look.color.as_str()) {
            return Err(said!(
                "settings.tabColor",
                "A tab's colour is one of {colors}.",
                colors = TAB_COLORS.join(", ")
            ));
        }
        let was_pinned = self.tab_looks.get(root).is_some_and(|known| known.pinned);
        if look.pinned && !was_pinned {
            // To the end of the pinned ones.
            if let Some(at) = self.tabs.iter().position(|tab| tab == root) {
                let tab = self.tabs.remove(at);
                let after = self.tabs.iter().take_while(|tab| self.is_pinned(tab)).count();
                self.tabs.insert(after, tab);
            }
        }
        if look == TabLook::default() {
            self.tab_looks.remove(root);
        } else {
            self.tab_looks.insert(root.to_owned(), look);
        }
        self.pinned_first();
        Ok(())
    }

    fn is_pinned(&self, root: &str) -> bool {
        self.tab_looks.get(root).is_some_and(|look| look.pinned)
    }

    /// The pinned tabs first, each group in its own order.
    pub fn pinned_first(&mut self) {
        let looks = &self.tab_looks;
        self.tabs.sort_by_key(|tab| !looks.get(tab).is_some_and(|look| look.pinned));
        for profile in &mut self.profiles {
            profile.tabs.sort_by_key(|tab| !looks.get(tab).is_some_and(|look| look.pinned));
        }
    }

    /// Every profile's name, in alphabetical order.
    pub fn profile_names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.profiles.iter().map(|profile| profile.name.clone()).chain([self.profile.clone()]).collect();
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    fn check_name(&self, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(said!("settings.profileName", "A profile's name is 1 to 40 characters."));
        }
        if self.profile_names().iter().any(|known| known.to_lowercase() == name.to_lowercase()) {
            return Err(said!("settings.profileExists", "There is a profile called {name} already.", name = name));
        }
        Ok(name.to_owned())
    }

    /// The profile in use, put away with its tabs.
    fn put_away(&mut self) -> Profile {
        Profile {
            name: std::mem::take(&mut self.profile),
            root: self.root.take(),
            tabs: std::mem::take(&mut self.tabs),
            recent: std::mem::take(&mut self.recent),
            appearance: std::mem::take(&mut self.appearance),
        }
    }

    fn take_up(&mut self, profile: Profile) {
        self.profile = profile.name;
        self.root = profile.root;
        self.tabs = profile.tabs;
        self.recent = profile.recent;
        self.appearance = profile.appearance;
    }

    /// The look of the profile in use; anything unknown is refused.
    pub fn set_appearance(&mut self, appearance: Appearance) -> Result<(), String> {
        if appearance.clone().checked() != appearance || appearance.mode.is_empty() {
            return Err(said!("settings.unknownLook", "That is not a look Haunt knows."));
        }
        self.appearance = appearance;
        Ok(())
    }

    /// Takes `root` off the recent list (its folder stays where it is).
    pub fn forget_recent(&mut self, root: &str) {
        self.recent.retain(|recent| recent != root);
    }

    /// Adds a folder for the repository manager to look through; an existing folder only, once.
    pub fn add_repository_folder(&mut self, path: &str) -> Result<(), String> {
        let folder = canonical(path).ok_or_else(|| said!("settings.folderMissing", "That folder is not there."))?;
        if !self.repository_folders.contains(&folder) {
            self.repository_folders.push(folder);
        }
        Ok(())
    }

    pub fn remove_repository_folder(&mut self, path: &str) {
        self.repository_folders.retain(|known| known != path);
    }

    /// Switches to the profile `name`, putting the one in use away with its tabs.
    pub fn switch_profile(&mut self, name: &str) -> Result<(), String> {
        if name == self.profile {
            return Ok(());
        }
        let at = self
            .profiles
            .iter()
            .position(|profile| profile.name == name)
            .ok_or_else(|| said!("settings.noProfile", "There is no such profile."))?;
        let next = self.profiles.remove(at);
        let current = self.put_away();
        self.profiles.push(current);
        self.take_up(next);
        Ok(())
    }

    /// A new profile with no tabs, switched to.
    pub fn new_profile(&mut self, name: &str) -> Result<(), String> {
        let name = self.check_name(name)?;
        let current = self.put_away();
        self.profiles.push(current);
        // A new profile starts with the look of the one in use, to change from there.
        let appearance = self.profiles.last().map(|current| current.appearance.clone()).unwrap_or_default();
        self.take_up(Profile { name, appearance, ..Profile::default() });
        Ok(())
    }

    pub fn rename_profile(&mut self, old: &str, new: &str) -> Result<(), String> {
        if old.trim() == new.trim() {
            return Ok(());
        }
        let new = self.check_name(new)?;
        if old == self.profile {
            self.profile = new;
        } else {
            let profile = self
                .profiles
                .iter_mut()
                .find(|profile| profile.name == old)
                .ok_or_else(|| said!("settings.noProfile", "There is no such profile."))?;
            profile.name = new;
        }
        Ok(())
    }

    /// Deletes a profile not in use (its folders stay where they are; only its tabs go).
    pub fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        if name == self.profile {
            return Err(said!("settings.switchFirst", "Switch to another profile before deleting this one."));
        }
        let before = self.profiles.len();
        self.profiles.retain(|profile| profile.name != name);
        if self.profiles.len() == before {
            return Err(said!("settings.noProfile", "There is no such profile."));
        }
        Ok(())
    }

    /// Moves the tab `root` to the profile `name`, as its last tab; returns what `close` does here.
    pub fn move_tab(&mut self, root: &str, name: &str) -> Result<Option<Option<String>>, String> {
        if name == self.profile {
            return Ok(None);
        }
        if !self.tabs.iter().any(|tab| tab == root) {
            return Err(said!("settings.notOpenTab", "That is not an open tab."));
        }
        let target = self
            .profiles
            .iter_mut()
            .find(|profile| profile.name == name)
            .ok_or_else(|| said!("settings.noProfile", "There is no such profile."))?;
        if !target.tabs.iter().any(|tab| tab == root) {
            target.tabs.push(root.to_owned());
        }
        target.root.get_or_insert_with(|| root.to_owned());
        target.recent.retain(|recent| recent != root);
        target.recent.insert(0, root.to_owned());
        target.recent.truncate(RECENT);
        self.recent.retain(|recent| recent != root);
        Ok(self.close(root))
    }

    pub fn save(&self, file: &Path) {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            if let Err(error) = fs::write(file, text) {
                eprintln!("[haunt] cannot save the settings: {error}");
            }
        }
    }

    /// `root` shown, in its tab (a new one at the end if it had none), and first among the recent ones.
    pub fn show(&mut self, root: &str) {
        self.root = Some(root.to_owned());
        if !self.tabs.iter().any(|tab| tab == root) {
            self.tabs.push(root.to_owned());
        }
        self.recent.retain(|recent| recent != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(RECENT);
    }

    /// Closes `root`'s tab; returns the tab to show next when it was the one shown (the one after it, else before it).
    pub fn close(&mut self, root: &str) -> Option<Option<String>> {
        let at = self.tabs.iter().position(|tab| tab == root)?;
        self.tabs.remove(at);
        if self.root.as_deref() != Some(root) {
            return None;
        }
        let next = self.tabs.get(at).or_else(|| at.checked_sub(1).and_then(|before| self.tabs.get(before))).cloned();
        self.root = next.clone();
        Some(next)
    }

    pub fn trusts(&self, root: &str) -> bool {
        self.trusted.iter().any(|path| path == root)
    }

    pub fn set_trust(&mut self, root: &str, trusted: bool) {
        self.trusted.retain(|path| path != root);
        if trusted {
            self.trusted.push(root.to_owned());
        }
    }
}

/// Where the settings live: the app's config folder, or HAUNT_SETTINGS when set (the tests use it).
pub fn file(config: PathBuf) -> PathBuf {
    std::env::var_os("HAUNT_SETTINGS").map(PathBuf::from).unwrap_or_else(|| config.join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here() -> String {
        canonical(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    #[test]
    fn tabs_open_close_and_hand_over() {
        let mut settings = Settings::default();
        settings.show("a");
        settings.show("b");
        settings.show("c");
        assert_eq!(settings.tabs, ["a", "b", "c"]);
        assert_eq!(settings.recent, ["c", "b", "a"]);
        // Showing a tab that is open keeps the order of the tabs.
        settings.show("a");
        assert_eq!(settings.tabs, ["a", "b", "c"]);
        // Closing the one shown hands over to the tab after it, else the one before.
        assert_eq!(settings.close("a"), Some(Some("b".into())));
        settings.show("c");
        assert_eq!(settings.close("c"), Some(Some("b".into())));
        // Closing another tab changes nothing shown.
        settings.show("d");
        assert_eq!(settings.close("b"), None);
        assert_eq!(settings.close("d"), Some(None));
        assert_eq!(settings.root, None);
        assert_eq!(settings.close("nope"), None);
    }

    #[test]
    fn pinned_tabs_come_first_and_colours_are_checked() {
        let mut settings = Settings::default();
        for tab in ["a", "b", "c", "d"] {
            settings.show(tab);
        }
        let pin = |color: &str| TabLook { pinned: true, color: color.into() };
        settings.set_tab_look("c", pin("teal")).unwrap();
        settings.set_tab_look("d", pin("")).unwrap();
        assert_eq!(settings.tabs, ["c", "d", "a", "b"]);
        // Unpinned, it keeps its colour and moves after the pinned ones.
        settings.set_tab_look("c", TabLook { pinned: false, color: "teal".into() }).unwrap();
        assert_eq!(settings.tabs, ["d", "c", "a", "b"]);
        assert!(settings.set_tab_look("a", TabLook { pinned: false, color: "chartreuse".into() }).is_err());
        // Back to plain: forgotten.
        settings.set_tab_look("c", TabLook::default()).unwrap();
        assert_eq!(settings.tab_looks.len(), 1);
        // A new tab opens after the pinned ones too; dragging cannot put it among them.
        settings.show("e");
        settings.tabs = vec!["a".into(), "d".into(), "e".into(), "c".into(), "b".into()];
        settings.pinned_first();
        assert_eq!(settings.tabs, ["d", "a", "e", "c", "b"]);
    }

    #[test]
    fn keeps_the_git_settings_and_drops_an_interval_that_is_not_a_choice() {
        let dir = std::env::temp_dir().join(format!("haunt-settings-git-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        let settings = Settings { pull: PullChoice::Rebase, fetch_every: 15, ..Settings::default() };
        settings.save(&file);
        let read = Settings::load(&file);
        assert_eq!((read.pull, read.fetch_every), (PullChoice::Rebase, 15));
        // A hand-edited file asking for every 7 minutes: off, not 7.
        fs::write(&file, r#"{ "pull": "merge", "fetch_every": 7 }"#).unwrap();
        let read = Settings::load(&file);
        assert_eq!((read.pull, read.fetch_every), (PullChoice::Merge, 0));
        // Settings from before these: ask, and no background fetch.
        fs::write(&file, r#"{ "tabs": [] }"#).unwrap();
        assert_eq!(Settings::load(&file).pull, PullChoice::Ask);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_old_and_broken_files() {
        let dir = std::env::temp_dir().join(format!("haunt-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        // Before the tabs: a root, recent ones (one twice, one missing), trusted ones.
        fs::write(
            &file,
            format!(
                r#"{{ "root": {0:?}, "recent": [{0:?}, {0:?}, "C:/no/such/folder"], "trusted": [{0:?}] }}"#,
                here()
            ),
        )
        .unwrap();
        let read = Settings::load(&file);
        assert_eq!(read.root, Some(here()));
        assert_eq!(read.tabs, [here()]);
        assert_eq!(read.recent, [here()]);
        assert!(read.trusts(&here()));
        fs::write(&file, "{ not json").unwrap();
        let fresh = Settings {
            profile: FIRST_PROFILE.into(),
            appearance: Appearance { theme: "haunt".into(), ..Appearance::default() },
            ..Settings::default()
        };
        assert_eq!(Settings::load(&file), fresh);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn profiles_keep_their_own_tabs() {
        let mut settings = Settings { profile: FIRST_PROFILE.into(), ..Settings::default() };
        settings.show("/a");
        settings.show("/b");
        settings.new_profile("Work").unwrap();
        assert_eq!((settings.profile.as_str(), settings.tabs.len(), settings.root.as_deref()), ("Work", 0, None));
        assert!(settings.new_profile("work").is_err(), "names are unique, whatever their case");
        assert!(settings.new_profile("  ").is_err());
        settings.show("/w");
        assert_eq!(settings.profile_names(), ["Default", "Work"]);

        settings.switch_profile("Default").unwrap();
        assert_eq!(
            (settings.tabs.clone(), settings.root.as_deref()),
            (vec!["/a".to_owned(), "/b".to_owned()], Some("/b"))
        );
        // Moving the tab shown to Work: Default shows the tab beside it, Work gets it at the end.
        assert_eq!(settings.move_tab("/b", "Work").unwrap(), Some(Some("/a".to_owned())));
        assert_eq!(settings.tabs, ["/a"]);
        settings.switch_profile("Work").unwrap();
        assert_eq!(settings.tabs, ["/w", "/b"]);
        assert_eq!(settings.root.as_deref(), Some("/w"));

        settings.forget_recent("/w");
        assert!(!settings.recent.contains(&"/w".to_owned()), "off the recent list");
        assert_eq!(settings.tabs, ["/w", "/b"], "its tab stays");
        settings.rename_profile("Default", "Own").unwrap();
        assert!(settings.delete_profile("Work").is_err(), "not the profile in use");
        settings.delete_profile("Own").unwrap();
        assert_eq!(settings.profile_names(), ["Work"]);
        assert!(settings.trusted.is_empty());
    }

    #[test]
    fn each_profile_has_its_own_look() {
        let mut settings = Settings { profile: FIRST_PROFILE.into(), ..Settings::default() };
        let blue = Appearance {
            mode: "dark".into(),
            primary: "blue".into(),
            neutral: "zinc".into(),
            dark_primary: "amber".into(),
            dark_neutral: "stone".into(),
            radius: "0.25".into(),
            theme: "cobalt".into(),
            ..Appearance::default()
        };
        settings.set_appearance(Appearance { dark_primary: SYSTEM_ACCENT.into(), ..blue.clone() }).unwrap();
        assert!(
            settings.set_appearance(Appearance { neutral: SYSTEM_ACCENT.into(), ..blue.clone() }).is_err(),
            "the accent is a main colour only"
        );
        settings.set_appearance(blue.clone()).unwrap();
        assert!(settings.set_appearance(Appearance { primary: "chartreuse".into(), ..blue.clone() }).is_err());
        // Any colour as a hex; kept as `#rrggbb` in lower case, and nothing that only looks like one.
        let hexes = Appearance {
            primary: "#2b7fff".into(),
            dark_neutral: "#141218".into(),
            error: "#f2b8b5".into(),
            ..blue.clone()
        };
        settings.set_appearance(hexes).unwrap();
        assert_eq!(hex("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(Appearance { neutral: "#D0BCFF".into(), ..Appearance::default() }.checked().neutral, "#d0bcff");
        for wrong in ["#12345", "#ggg", "2b7fff", "#2b7fff00"] {
            assert!(settings.set_appearance(Appearance { primary: wrong.into(), ..blue.clone() }).is_err(), "{wrong}");
        }
        settings.set_appearance(blue.clone()).unwrap();
        settings.new_profile("Work").unwrap();
        assert_eq!(settings.appearance, blue, "a new profile starts with the look in use");
        let light = Appearance { mode: "light".into(), primary: "rose".into(), ..blue.clone() };
        settings.set_appearance(light.clone()).unwrap();
        settings.switch_profile("Default").unwrap();
        assert_eq!(settings.appearance, blue);
        settings.switch_profile("Work").unwrap();
        assert_eq!(settings.appearance, light);
    }

    #[test]
    fn a_look_leaves_what_it_does_not_know_to_its_theme() {
        let old: Appearance = serde_json::from_str(
            r#"{ "mode": "dark", "primary": "teal", "neutral": "slate", "radius": "0", "font": "Comic Sans" }"#,
        )
        .unwrap();
        let look = old.checked();
        assert_eq!(look.theme, "haunt", "a look from before themes is Haunt's own");
        assert_eq!((look.primary.as_str(), look.neutral.as_str(), look.radius.as_str()), ("teal", "slate", "0"));
        assert_eq!((look.dark_primary.as_str(), look.font.as_str()), ("", ""), "empty is the theme's");
    }
}
