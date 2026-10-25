//! The system's notifications: a long run (a fetch, pull or push, the tests) that finishes while Haunt's window is not
//! in front says so, through the system (tauri-plugin-notification). With the window in front, nothing is sent: the
//! window says it itself.

use std::sync::OnceLock;

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Called once at start, with the app.
pub fn start(app: AppHandle) {
    let _ = APP.set(app);
}

/// Sends `title` and `body` as a system notification when the window is not in front.
pub fn notify_if_away(title: &str, body: &str) {
    let Some(app) = APP.get() else { return };
    let focused = app.get_webview_window("main").and_then(|window| window.is_focused().ok()).unwrap_or(false);
    if focused {
        return;
    }
    let title: String = title.chars().take(120).collect();
    let body: String = body.chars().take(300).collect();
    let _ = app.notification().builder().title(title).body(body).show();
}
