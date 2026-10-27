//! Git's questions (a password, a key's passphrase, whether to trust a host key) asked in Haunt's window.
//! Fetch, pull and push run git with `GIT_ASKPASS` and `SSH_ASKPASS` pointing
//! at Haunt's own program; started that way (`main.rs`), Haunt only passes the question on and prints the answer:
//!
//! - The app listens on a loopback port with a secret, both handed to git in `HAUNT_ASKPASS` (`env`), and to no one
//!   else.
//! - The asking program sends one line of JSON (the secret, the repository, git's question) and waits; the window gets
//!   the news `ask`, and `answer` sends back what was typed, or nothing when cancelled (git then stops).
//! - Nothing is kept: the answer goes straight to git. A question unanswered after five minutes is cancelled.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::events::Events;

/// How long a question waits for its answer.
const WAIT: Duration = Duration::from_secs(5 * 60);
/// The variable that tells Haunt's program it was started by git to ask something: "port:secret".
pub const VARIABLE: &str = "HAUNT_ASKPASS";

struct Asker {
    port: u16,
    secret: String,
    events: Arc<Events>,
    waiting: Mutex<HashMap<u64, Sender<Option<String>>>>,
    next: AtomicU64,
}

static ASKER: OnceLock<Arc<Asker>> = OnceLock::new();

/// A secret no other program can guess: four of the standard library's randomly seeded hashes.
fn secret() -> String {
    (0..4)
        .map(|round| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(round);
            hasher.write_u128(
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_nanos()).unwrap_or(0),
            );
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

/// Starts listening, once; questions are sent to the window as `events`.
pub fn start(events: Arc<Events>) -> std::io::Result<()> {
    if ASKER.get().is_some() {
        return Ok(());
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let asker = Arc::new(Asker {
        port: listener.local_addr()?.port(),
        secret: secret(),
        events,
        waiting: Mutex::default(),
        next: AtomicU64::new(1),
    });
    if ASKER.set(asker.clone()).is_err() {
        return Ok(());
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asker = asker.clone();
            std::thread::spawn(move || {
                let _ = asker.serve(stream);
            });
        }
    });
    Ok(())
}

/// What git needs to ask through Haunt, for the repository `repo`; nothing when Haunt is not listening (in tests): git
/// then cannot ask, and stops with its own message.
pub fn env(repo: &str) -> Vec<(String, String)> {
    let (Some(asker), Ok(program)) = (ASKER.get(), std::env::current_exe()) else {
        return Vec::new();
    };
    let program = program.to_string_lossy().replace('\\', "/");
    vec![
        ("GIT_ASKPASS".into(), program.clone()),
        ("SSH_ASKPASS".into(), program),
        ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
        (VARIABLE.into(), format!("{}:{}", asker.port, asker.secret)),
        ("HAUNT_ASKPASS_REPO".into(), repo.into()),
    ]
}

/// The window's answer to question `id`: the text, or null to cancel.
pub fn answer(id: u64, text: Option<String>) {
    if let Some(waiting) = ASKER.get().and_then(|asker| asker.waiting.lock().remove(&id)) {
        let _ = waiting.send(text);
    }
}

/// Whether a question wants a secret typed (shown as dots) and whether it is a yes or no.
fn kind(prompt: &str) -> (bool, bool) {
    let lower = prompt.to_lowercase();
    let secret = ["password", "passphrase", "token", "pin "].iter().any(|word| lower.contains(word));
    (secret, lower.contains("(yes/no"))
}

impl Asker {
    fn serve(&self, stream: TcpStream) -> std::io::Result<()> {
        #[derive(Deserialize)]
        struct Question {
            secret: String,
            repo: Option<String>,
            prompt: String,
        }
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut line = String::new();
        BufReader::new(&stream).take_line(&mut line)?;
        let Ok(question) = serde_json::from_str::<Question>(&line) else {
            return Ok(());
        };
        // Compared in full whatever the first difference, so the time taken says nothing about the secret.
        let same = question.secret.len() == self.secret.len()
            && question.secret.bytes().zip(self.secret.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
        if !same {
            return Ok(());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = channel();
        self.waiting.lock().insert(id, sender);
        let (secret, yes_no) = kind(&question.prompt);
        let prompt = question.prompt.trim().to_owned();
        self.events.send(
            question.repo.as_deref(),
            json!({ "type": "ask", "id": id, "prompt": prompt, "secret": secret, "yesNo": yes_no }),
        );
        let answer = receiver.recv_timeout(WAIT).unwrap_or(None);
        if self.waiting.lock().remove(&id).is_some() {
            // Not answered in time: the window closes the question.
            self.events.send(question.repo.as_deref(), json!({ "type": "asked", "id": id }));
        }
        let mut out = &stream;
        writeln!(out, "{}", json!({ "answer": answer }))?;
        out.flush()
    }
}

trait TakeLine {
    fn take_line(&mut self, line: &mut String) -> std::io::Result<()>;
}

impl<R: std::io::Read> TakeLine for BufReader<R> {
    /// One line, at most 64 KB.
    fn take_line(&mut self, line: &mut String) -> std::io::Result<()> {
        use std::io::Read;
        self.by_ref().take(64 * 1024).read_line(line).map(drop)
    }
}

/// Asks the app listening at `address` ("port:secret") and returns the answer; None when cancelled or unreachable.
pub fn ask(address: &str, repo: Option<&str>, prompt: &str) -> Option<String> {
    let (port, secret) = address.split_once(':')?;
    let port: u16 = port.parse().ok()?;
    let stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(WAIT + Duration::from_secs(30))).ok()?;
    let mut out = &stream;
    writeln!(out, "{}", json!({ "secret": secret, "repo": repo, "prompt": prompt })).ok()?;
    out.flush().ok()?;
    let mut line = String::new();
    BufReader::new(&stream).take_line(&mut line).ok()?;
    let answer: serde_json::Value = serde_json::from_str(&line).ok()?;
    answer["answer"].as_str().map(str::to_owned)
}

/// Haunt's program started by git or ssh to ask something (`HAUNT_ASKPASS` set, the question as its one argument):
/// prints the answer and returns the exit code, 0 answered and 1 cancelled. None when Haunt was started normally.
pub fn main() -> Option<i32> {
    let address = std::env::var(VARIABLE).ok()?;
    let mut args = std::env::args().skip(1);
    let prompt = args.next()?;
    if args.next().is_some() {
        return None;
    }
    // Empty for git run where no tab is (a clone): the window asks whatever it shows.
    let repo = std::env::var("HAUNT_ASKPASS_REPO").ok().filter(|repo| !repo.is_empty());
    Some(match ask(&address, repo.as_deref(), &prompt) {
        Some(answer) => {
            println!("{answer}");
            0
        }
        None => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_secrets_and_yes_or_no_questions_apart() {
        assert_eq!(kind("Password for 'https://me@example.com': "), (true, false));
        assert_eq!(kind("Enter passphrase for key '/home/me/.ssh/id_ed25519': "), (true, false));
        assert_eq!(kind("Username for 'https://example.com': "), (false, false));
        assert_eq!(kind("Are you sure you want to continue connecting (yes/no/[fingerprint])? "), (false, true));
    }

    #[test]
    fn a_question_goes_to_the_window_and_its_answer_back() {
        let events = Arc::new(Events::default());
        let news = events.listen();
        start(events).unwrap();
        let address = env("/repo").into_iter().find(|(name, _)| name == VARIABLE).unwrap().1;

        let asking = std::thread::spawn({
            let address = address.clone();
            move || ask(&address, Some("/repo"), "Password for 'https://example.com': ")
        });
        let question: serde_json::Value =
            serde_json::from_str(&news.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
        assert_eq!(question["type"], "ask");
        assert_eq!(question["repo"], "/repo");
        assert_eq!(question["secret"], true);
        answer(question["id"].as_u64().unwrap(), Some("hunter2".into()));
        assert_eq!(asking.join().unwrap().as_deref(), Some("hunter2"));

        // Cancelled: no answer, so git stops.
        let asking = std::thread::spawn({
            let address = address.clone();
            move || ask(&address, None, "Username for 'https://example.com': ")
        });
        let question: serde_json::Value =
            serde_json::from_str(&news.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
        answer(question["id"].as_u64().unwrap(), None);
        assert_eq!(asking.join().unwrap(), None);

        // A wrong secret gets nothing, and the window hears nothing.
        let port = address.split_once(':').unwrap().0;
        assert_eq!(ask(&format!("{port}:wrong"), None, "Password: "), None);
        assert!(news.recv_timeout(Duration::from_millis(200)).is_err());
    }
}
