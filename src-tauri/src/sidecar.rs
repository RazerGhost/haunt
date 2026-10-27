//! The TypeScript scan's process: `node sidecar/main.ts <repository>` (or Bun or Deno: startup.rs), one per shown
//! repository, spoken to over stdin and stdout with one JSON message per line (sidecar/serve.ts has the other side).
//! Replies and scans pass through as raw JSON text: Rust reads only the few fields it needs (what to watch, which
//! files the scan found).
//!
//! The sidecar asks too, while it scans: `{ "ask": n, "type", ... }` for what Rust reads itself (PHP, Rust and Lua),
//! answered on its stdin with `{ "answer": n, "ok", "value" }` or `{ "answer": n, "ok": false, "error" }`.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;

use crate::startup::Runtime;

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

/// What the sidecar says unasked.
pub enum Said {
    /// A new structure: the scan as JSON text (calls still to come), and its shape.
    Structure(String, Shape),
    /// The call graph for a structure: the whole message as JSON text (`scannedAt`, `calls`, `ms`).
    Calls(String, String),
    Error(String),
    /// The process ended.
    Exited,
}

type Reply = Result<String, String>;

/// Answers what the sidecar asks: the whole line in, the answer's value as JSON text out.
pub type Answerer = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

pub struct Sidecar {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, Sender<Reply>>>>,
    next: AtomicU64,
}

#[derive(Deserialize)]
struct Line<'a> {
    id: Option<u64>,
    ask: Option<u64>,
    ok: Option<bool>,
    #[serde(borrow)]
    value: Option<&'a RawValue>,
    error: Option<String>,
    event: Option<String>,
    #[serde(borrow)]
    scan: Option<&'a RawValue>,
    message: Option<String>,
    #[serde(rename = "scannedAt")]
    scanned_at: Option<String>,
}

impl Sidecar {
    /// Starts the process and a thread that reads what it says; `said` runs on that thread.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        runtime: &Runtime,
        script: &Path,
        root: &str,
        trusted: bool,
        log: Option<&Path>,
        cache: Option<&Path>,
        said: impl Fn(Said) + Send + 'static,
        answer: Answerer,
    ) -> std::io::Result<Sidecar> {
        let mut command = runtime.command(script);
        command
            .arg(root)
            .current_dir(script.parent().and_then(Path::parent).unwrap_or(Path::new(".")))
            .env("HAUNT_TRUSTED", if trusted { "1" } else { "0" })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(match log.and_then(|path| File::options().create(true).append(true).open(path).ok()) {
                Some(file) => Stdio::from(file),
                None => Stdio::inherit(),
            });
        if let Some(cache) = cache {
            // Where the sidecar keeps each repository's call graph between launches (sidecar/serve.ts).
            command.env("HAUNT_CACHE", cache);
        }
        crate::startup::hide_console(&mut command);
        let mut child = command.spawn()?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
        let answers = stdin.clone();
        let stdout = child.stdout.take().expect("piped stdout");
        let pending: Arc<Mutex<HashMap<u64, Sender<Reply>>>> = Arc::default();
        let waiting = pending.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(parsed) = serde_json::from_str::<Line>(&line) else { continue };
                if let Some(ask) = parsed.ask {
                    // On its own thread: reading what the sidecar says goes on meanwhile.
                    let (answer, answers) = (answer.clone(), answers.clone());
                    std::thread::spawn(move || {
                        let reply = match answer(&line) {
                            Ok(value) => format!("{{\"answer\":{ask},\"ok\":true,\"value\":{value}}}"),
                            Err(error) => serde_json::json!({ "answer": ask, "ok": false, "error": error }).to_string(),
                        };
                        let _ = writeln!(answers.lock(), "{reply}");
                    });
                    continue;
                }
                if let Some(id) = parsed.id {
                    let reply = if parsed.ok == Some(true) {
                        Ok(parsed.value.map(|raw| raw.get().to_owned()).unwrap_or_else(|| "null".into()))
                    } else {
                        Err(parsed.error.unwrap_or_else(|| said!("sidecar.noAnswer", "The scanner could not answer.")))
                    };
                    if let Some(sender) = waiting.lock().remove(&id) {
                        let _ = sender.send(reply);
                    }
                    continue;
                }
                match parsed.event.as_deref() {
                    Some("structure") => {
                        if let Some(raw) = parsed.scan {
                            let shape = serde_json::from_str::<Shape>(raw.get()).unwrap_or_default();
                            said(Said::Structure(raw.get().to_owned(), shape));
                        }
                    }
                    Some("calls") => said(Said::Calls(line.clone(), parsed.scanned_at.unwrap_or_default())),
                    Some("error") => said(Said::Error(parsed.message.unwrap_or_default())),
                    _ => {}
                }
            }
            // stdout closed: the process is gone. Whoever still waits for an answer hears so now, not after a timeout.
            for (_, sender) in waiting.lock().drain() {
                let _ = sender.send(Err(said!("sidecar.stoppedEarly", "The scanner stopped before it answered.")));
            }
            said(Said::Exited);
        });
        Ok(Sidecar { child: Mutex::new(child), stdin, pending, next: AtomicU64::new(1) })
    }

    /// Sends a message that has no reply (`scan`).
    pub fn tell(&self, message: &Value) {
        let _ = writeln!(self.stdin.lock(), "{message}");
    }

    /// Sends a request; the receiver gets the reply's value as JSON text, or its error.
    pub fn ask(&self, mut request: Value) -> Receiver<Reply> {
        let (sender, receiver) = channel();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        request["id"] = id.into();
        self.pending.lock().insert(id, sender);
        if writeln!(self.stdin.lock(), "{request}").is_err() {
            if let Some(sender) = self.pending.lock().remove(&id) {
                let _ = sender.send(Err(said!("sidecar.notRunning", "The scanner is not running.")));
            }
        }
        receiver
    }

    /// Whether the process is still running.
    pub fn running(&self) -> bool {
        self.child.lock().try_wait().is_ok_and(|status| status.is_none())
    }

    pub fn stop(&self) {
        let _ = self.child.lock().kill();
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.stop();
    }
}
