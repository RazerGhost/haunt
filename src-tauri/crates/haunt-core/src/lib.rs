//! Haunt's core, without the app: reading and writing git repositories (git/), reading code by function and the scan
//! done without a JavaScript runtime (code/), and what they are built on: Haunt's texts for the window (said.rs),
//! paths (paths.rs), and starting programs with a time limit (process.rs, appimage.rs). Nothing here knows Tauri or
//! the window; the app (src-tauri) hands in what it must, such as askpass's environment (`git::command::set_askpass`).

// First: the said! macro is used by the modules after it.
#[macro_use]
pub mod said;
pub mod appimage;
pub mod code;
pub mod git;
pub mod paths;
pub mod process;
