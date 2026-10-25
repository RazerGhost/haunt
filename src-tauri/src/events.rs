//! News for the window: a repository was shown, a scan started or came in, git changed, a scan failed. Every message
//! names its repository, so a window that has moved on to another tab can drop it. lib.rs attaches the sender that
//! makes it the Tauri event `haunt`; while Haunt is worked on, the development bridge's listeners get it too. (No Tauri
//! types here, so the tests run without a window.)

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

use serde_json::Value;

type Sink = Box<dyn Fn(&Value) + Send + Sync>;

#[derive(Default)]
pub struct Events {
    window: Mutex<Option<Sink>>,
    listeners: Mutex<Vec<Sender<String>>>,
}

impl Events {
    /// How news reaches the window.
    pub fn attach(&self, window: Sink) {
        *self.window.lock().unwrap() = Some(window);
    }

    /// Sends `news` (an object with a `type`) for `repo`.
    pub fn send(&self, repo: Option<&str>, mut news: Value) {
        news["repo"] = repo.map(Value::from).unwrap_or(Value::Null);
        if let Some(window) = self.window.lock().unwrap().as_ref() {
            window(&news);
        }
        let text = news.to_string();
        self.listeners.lock().unwrap().retain(|listener| listener.send(text.clone()).is_ok());
    }

    /// A listener for the development bridge: every message, as JSON text.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn listen(&self) -> Receiver<String> {
        let (sender, receiver) = channel();
        self.listeners.lock().unwrap().push(sender);
        receiver
    }
}
