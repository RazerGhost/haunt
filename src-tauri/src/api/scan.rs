//! The latest scan, as commands read it: its files for the changes by function, and a scanned file's lines.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::Api;
use crate::code::changes::ScannedFile;

/// Lines of a scanned file as it is now.
#[derive(Debug, Serialize, Type)]
pub struct Source {
    pub start: u32,
    pub lines: Vec<String>,
    pub cut: bool,
}

impl Api {
    /// The files of the latest scan, by path, for the changes by function. A scan can be several megabytes of JSON:
    /// it is parsed once per scan (by repository and the scan's time) and kept.
    pub(super) fn scanned_files(&self, repo: &str) -> Result<Arc<HashMap<String, ScannedFile>>, String> {
        #[derive(Deserialize)]
        struct Scan {
            files: Vec<ScannedFile>,
        }
        type Kept = Option<(String, String, Arc<HashMap<String, ScannedFile>>)>;
        static KEPT: std::sync::OnceLock<parking_lot::Mutex<Kept>> = std::sync::OnceLock::new();
        let text = self
            .repos
            .structure(repo)?
            .ok_or_else(|| said!("api.stillScanning", "Haunt is still scanning this repository."))?;
        // "scannedAt":"2026-10-25T…" sits near the start of the scan's JSON.
        let scanned_at = text
            .find("\"scannedAt\":\"")
            .and_then(|at| text[at + 13..].split('"').next())
            .unwrap_or_default()
            .to_owned();
        let kept = KEPT.get_or_init(Default::default);
        if let Some((known, at, files)) = kept.lock().as_ref() {
            if known == repo && !scanned_at.is_empty() && *at == scanned_at {
                return Ok(files.clone());
            }
        }
        let scan: Scan = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let files: Arc<HashMap<String, ScannedFile>> =
            Arc::new(scan.files.into_iter().map(|file| (file.path.clone(), file)).collect());
        *kept.lock() = Some((repo.to_owned(), scanned_at, files.clone()));
        Ok(files)
    }

    /// Lines `start` to `end` of a scanned file as it is now, for the function panel; at most 400.
    pub(super) fn source(&self, repo: &str, path: &str, start: u32, end: u32) -> Result<Source, String> {
        const MAX_LINES: u32 = 400;
        if start < 1 || end < start {
            return Err(said!("api.functionNotScanned", "That function is not in the scan."));
        }
        let file = self.repos.scanned(repo, path)?;
        let text =
            std::fs::read_to_string(&file).map_err(|_| said!("api.fileUnread", "The file could not be read."))?;
        let all: Vec<&str> = text.lines().collect();
        let last = end.min(all.len() as u32).min(start + MAX_LINES - 1);
        let lines = all
            .get(start as usize - 1..last as usize)
            .map(|lines| lines.iter().map(|line| line.to_string()).collect())
            .unwrap_or_default();
        Ok(Source { start, lines, cut: last < end })
    }
}
