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
            let wanted = inner.settings.root.clone().filter(|root| paths::openable(Path::new(root)));
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

    /// Puts the tabs in a new order (the same tabs, moved).
    pub fn reorder(&self, tabs: &[String]) -> Result<RootInfo, String> {
        {
            let mut inner = self.inner.lock();
            let mut wanted: Vec<&String> = tabs.iter().collect();
            let mut known: Vec<&String> = inner.settings.tabs.iter().collect();
            wanted.sort();
            known.sort();
            if wanted != known {
                return Err(said!("repos.tabsChanged", "The tabs changed meanwhile."));
            }
            inner.settings.tabs = tabs.to_vec();
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
                    match next.filter(|next| paths::openable(Path::new(next))) {
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
