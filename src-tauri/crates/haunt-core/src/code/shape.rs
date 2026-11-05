//! The few fields of a scan that Rust reads itself, from the scanner's (sidecar/) and the one done here (`scan.rs`)
//! alike.

use serde::Deserialize;

/// What the window and the watchers need of a scan, read without taking the rest apart.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shape {
    pub scanned_at: String,
    /// Folders, relative to the root, the scanned files are in.
    pub watch: Vec<String>,
    /// Folders whose change needs a new process (the project's rules in code).
    pub restart_on: Vec<String>,
    pub files: Vec<ShapeFile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShapeFile {
    pub path: String,
}
