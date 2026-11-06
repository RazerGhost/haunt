//! The tabs and the profiles: opening, closing and ordering tabs, and switching or changing profiles.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{Repos, RootInfo};
use crate::paths;

impl Repos {
    /// Changes the profiles (`settings.rs`); when that changes the repository to show (another profile, or its tab
    /// moved to another one), shows it, or nothing.
    pub fn change_profiles(
        &self,
        change: impl FnOnce(&mut crate::settings::Settings) -> Result<(), String>,
    ) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock();
            change(&mut inner.settings)?;
            let wanted = inner
                .settings
                .root
                .clone()
                .filter(|root| paths::openable(Path::new(root)))
                .or_else(|| first_openable(&inner.settings.tabs));
            if wanted.is_some() {
                inner.settings.root = wanted.clone();
            }
            let shown = inner.shown.as_ref().map(|shown| shown.root.clone());
            if wanted != shown {
                match wanted {
                    Some(root) => self.show(&mut inner, &root),
                    None => {
                        inner.shown = None;
                        self.events.send(None, json!({ "type": "root", "root": null }));
                    }
                }
            }
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// Shows another repository, in its tab (a new tab if it has none), and remembers it.
    pub fn open(&self, path: &str) -> Result<RootInfo, String> {
        let root = paths::canonical(path)
            .ok_or_else(|| said!("repos.notFound", "{path} is not a folder Haunt can find.", path = path))?;
        if !paths::openable(Path::new(&root)) {
            return Err(said!("repos.notProject", "{root} is not a project or a git repository.", root = root));
        }
        {
            let mut inner = self.inner.lock();
            if inner.shown.as_ref().map(|shown| shown.root.as_str()) != Some(root.as_str()) {
                self.show(&mut inner, &root);
            }
            inner.settings.show(&root);
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// Puts the tabs in a new order (the same tabs, moved). The window only has the tabs whose folder is there; the
    /// others (a folder moved, a drive unplugged) keep their places.
    pub fn reorder(&self, tabs: &[String]) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock();
            let order = reordered(&inner.settings.tabs, |tab| paths::openable(Path::new(tab)), tabs)
                .ok_or_else(|| said!("repos.tabsChanged", "The tabs changed meanwhile."))?;
            inner.settings.tabs = order;
            // A tab dragged among the pinned ones stays unpinned, after them.
            inner.settings.pinned_first();
            inner.settings.save(&self.context.settings);
        }
        Ok(self.info())
    }

    /// An open tab's folder, or a recent one's, for the file manager; any other path is refused.
    pub fn tab_folder(&self, path: &str) -> Result<PathBuf, String> {
        let inner = self.inner.lock();
        let known = inner.settings.tabs.iter().chain(inner.settings.recent.iter()).find(|tab| *tab == path);
        if let Some(known) = known {
            return Ok(PathBuf::from(known));
        }
        // A repository in one of the repository manager's folders.
        let root = paths::canonical(path).filter(|root| {
            inner.settings.repository_folders.iter().any(|folder| Path::new(root).starts_with(folder))
                && Path::new(root).join(".git").exists()
        });
        root.map(PathBuf::from)
            .ok_or_else(|| said!("repos.notListed", "That is not an open tab or a repository Haunt lists."))
    }

    /// Closes a tab; closing the one shown shows the tab next to it, or none.
    pub fn close(&self, path: &str) -> RootInfo {
        let root = paths::canonical(path).unwrap_or_else(|| path.to_owned());
        {
            let mut inner = self.inner.lock();
            inner.kept.retain(|kept| kept.root != root);
            crate::code::others::forget(&root);
            crate::git::forget_config(&root);
            crate::git::objects::forget(&root);
            crate::git::write::forget(&root);
            match inner.settings.close(&root) {
                None => {}
                Some(next) => {
                    inner.shown = None;
                    inner.kept.retain(|kept| kept.root != root);
                    // The tab beside it may be one whose folder is missing: then the first one that is there.
                    let next = next
                        .filter(|next| paths::openable(Path::new(next)))
                        .or_else(|| first_openable(&inner.settings.tabs));
                    inner.settings.root = next.clone();
                    match next {
                        Some(next) => self.show(&mut inner, &next),
                        None => self.events.send(None, json!({ "type": "root", "root": null })),
                    }
                }
            }
            inner.settings.save(&self.context.settings);
        }
        self.info()
    }
}

/// The first tab whose folder is there.
pub(super) fn first_openable(tabs: &[String]) -> Option<String> {
    tabs.iter().find(|tab| paths::openable(Path::new(tab))).cloned()
}

/// `tabs` with the ones that are `there` put in the order of `wanted`, and the others left in their places; None when
/// `wanted` is not the same tabs as those that are there.
fn reordered(tabs: &[String], there: impl Fn(&str) -> bool, wanted: &[String]) -> Option<Vec<String>> {
    let there: Vec<bool> = tabs.iter().map(|tab| there(tab)).collect();
    let mut shown: Vec<&String> = tabs.iter().zip(&there).filter(|(_, there)| **there).map(|(tab, _)| tab).collect();
    let mut asked: Vec<&String> = wanted.iter().collect();
    shown.sort();
    asked.sort();
    if shown != asked {
        return None;
    }
    let mut wanted = wanted.iter();
    tabs.iter()
        .zip(&there)
        .map(|(tab, there)| if *there { wanted.next().cloned() } else { Some(tab.clone()) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::reordered;

    fn tabs(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_missing_tab_keeps_its_place_when_the_others_are_dragged() {
        let all = tabs(&["a", "gone", "b", "c"]);
        let there = |tab: &str| tab != "gone";
        assert_eq!(reordered(&all, there, &tabs(&["c", "a", "b"])), Some(tabs(&["c", "gone", "a", "b"])));
        assert_eq!(reordered(&all, there, &tabs(&["a", "b", "c"])), Some(all.clone()));
        // Not the tabs that are there: one missing, one too many, one unknown.
        assert_eq!(reordered(&all, there, &tabs(&["a", "b"])), None);
        assert_eq!(reordered(&all, there, &tabs(&["a", "b", "c", "gone"])), None);
        assert_eq!(reordered(&all, there, &tabs(&["a", "b", "d"])), None);
    }
}
