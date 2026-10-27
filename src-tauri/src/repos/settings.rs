//! The Settings window's side: the git settings, the switches for outside services, and Haunt's own folders.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::Repos;
use crate::settings::{PullChoice, FETCH_EVERY};

/// What the Settings window shows: the git settings, the trusted folders, and where Haunt keeps its files.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub pull: PullChoice,
    /// Minutes between background fetches; 0 is off.
    pub fetch_every: u32,
    /// Pull request status from gh, glab or tea; off by default.
    pub host_tools: bool,
    /// Looking for a new version of Haunt now and then; off by default.
    pub update_check: bool,
    pub trusted: Vec<TrustedFolder>,
    pub version: String,
    pub folders: AppFolders,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TrustedFolder {
    pub path: String,
    /// The folder is not there now (an unplugged drive, a deleted checkout).
    pub missing: bool,
}

/// Where Haunt keeps its files; null where there is none (the logs while Haunt is worked on).
#[derive(Debug, Clone, Serialize, Type)]
pub struct AppFolders {
    pub settings: String,
    pub cache: Option<String>,
    pub logs: Option<String>,
}

/// One of Haunt's own folders, to open in the file manager.
#[derive(Debug, Clone, Copy, serde::Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AppFolder {
    Settings,
    Cache,
    Logs,
}

impl Repos {
    /// The git settings, the trusted folders and Haunt's own folders, for the Settings window.
    pub fn app_settings(&self) -> AppSettings {
        let inner = self.inner.lock();
        let text = |path: &Path| path.to_string_lossy().into_owned();
        AppSettings {
            pull: inner.settings.pull,
            fetch_every: inner.settings.fetch_every,
            host_tools: inner.settings.host_tools,
            update_check: inner.settings.update_check,
            trusted: inner
                .settings
                .trusted
                .iter()
                .map(|path| TrustedFolder { path: path.clone(), missing: !Path::new(path).is_dir() })
                .collect(),
            version: env!("CARGO_PKG_VERSION").into(),
            folders: AppFolders {
                settings: self.context.settings.parent().map(text).unwrap_or_default(),
                cache: self.context.cache.as_deref().map(text),
                logs: self.context.logs.as_deref().map(text),
            },
        }
    }

    /// What a pull does when the branches went apart, and how often to fetch in the background (0, 5, 15 or 30).
    pub fn set_git(&self, pull: PullChoice, fetch_every: u32) -> Result<AppSettings, String> {
        if !FETCH_EVERY.contains(&fetch_every) {
            return Err(said!(
                "repos.fetchChoice",
                "Fetching every {minutes} minutes is not one of the choices.",
                minutes = fetch_every
            ));
        }
        {
            let mut inner = self.inner.lock();
            inner.settings.pull = pull;
            inner.settings.fetch_every = fetch_every;
            inner.settings.save(&self.context.settings);
        }
        Ok(self.app_settings())
    }

    /// The switches for what Haunt may ask outside services: pull request status through the host's tool, and the update
    /// check.
    pub fn set_online(&self, host_tools: bool, update_check: bool) -> AppSettings {
        {
            let mut inner = self.inner.lock();
            inner.settings.host_tools = host_tools;
            inner.settings.update_check = update_check;
            inner.settings.save(&self.context.settings);
        }
        self.app_settings()
    }

    pub fn host_tools(&self) -> bool {
        self.inner.lock().settings.host_tools
    }

    /// Removes the call graphs the scanners keep between launches; how many there were.
    pub fn clear_call_graphs(&self) -> Result<u32, String> {
        let Some(dir) = &self.context.cache else { return Ok(0) };
        let mut removed = 0;
        for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())?.flatten() {
            if entry.path().is_file() && std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// One of Haunt's own folders, for the file manager.
    pub fn folder(&self, which: AppFolder) -> Option<PathBuf> {
        match which {
            AppFolder::Settings => self.context.settings.parent().map(Path::to_path_buf),
            AppFolder::Cache => self.context.cache.clone(),
            AppFolder::Logs => self.context.logs.clone(),
        }
    }

    /// How often to fetch in the background (minutes, 0 off) and the repository shown, for the background fetch.
    pub fn fetch_schedule(&self) -> (u32, Option<String>) {
        let inner = self.inner.lock();
        (inner.settings.fetch_every, inner.shown.as_ref().map(|shown| shown.root.clone()))
    }

    /// The app's config folder (where the settings are), for what Haunt keeps beside them.
    pub fn config_dir(&self) -> PathBuf {
        self.context.settings.parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// Node, for running the project's tests; or why it cannot be used. Looked for each time: Bun or Deno may run
    /// the scanner while Node is installed later.
    pub fn node(&self) -> Result<std::path::PathBuf, String> {
        crate::startup::find_node()
    }
}
