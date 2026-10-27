//! The functions of other versions of a file, for the changes by function and a function's history.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use super::Api;
use crate::code::languages::{language_of, Language};
use crate::code::types::FunctionNode;
use crate::repos::Reply;

/// How long a call waits for the scanner (the first scan of a big repository with its call graph).
const WAIT: Duration = Duration::from_secs(120);

fn wait(receiver: Receiver<Reply>) -> Reply {
    receiver
        .recv_timeout(WAIT)
        .unwrap_or_else(|_| Err(said!("api.scannerTooSlow", "The scanner took too long to answer.")))
}

impl Api {
    /// The functions of a version of a file: the TypeScript family from the reader (sidecar/read.ts), the others read
    /// here; an error when the reader failed or took too long.
    fn try_read_version(&self, repo: &str, path: &str, text: &str) -> Result<Vec<FunctionNode>, String> {
        if language_of(path) != Some(Language::Typescript) {
            return Ok(crate::code::others::functions_of(repo, path, text));
        }
        self.repos
            .read_functions(repo, path, text)
            .and_then(wait)
            .and_then(|reply| serde_json::from_str(&reply).map_err(|error| error.to_string()))
    }

    /// The same, with no functions when it could not be read (a function's history, where a miss only ends the trail).
    pub(super) fn read_version(&self, repo: &str, path: &str, text: &str) -> Vec<FunctionNode> {
        self.try_read_version(repo, path, text).unwrap_or_default()
    }

    /// Works out changes by function with `work`, then marks the files a version of which could not be read: their
    /// functions are left out (every one would otherwise look added or removed).
    pub(super) fn by_function<T>(
        &self,
        repo: &str,
        work: impl FnOnce(&crate::code::changes::ReadVersion) -> Result<T, String>,
        files_of: impl FnOnce(&mut T) -> Option<&mut Vec<crate::code::changes::ChangedFile>>,
    ) -> Result<T, String> {
        let failed = std::cell::RefCell::new(std::collections::HashSet::new());
        let read = |path: &str, text: &str| match self.try_read_version(repo, path, text) {
            Ok(functions) => functions,
            Err(_) => {
                failed.borrow_mut().insert(path.to_owned());
                Vec::new()
            }
        };
        let mut result = work(&read)?;
        let failed = failed.into_inner();
        if !failed.is_empty() {
            if let Some(files) = files_of(&mut result) {
                for file in
                    files.iter_mut().filter(|file| failed.contains(&file.path) || failed.contains(&file.old_path))
                {
                    file.unread = true;
                    file.functions.clear();
                }
            }
        }
        Ok(result)
    }
}
