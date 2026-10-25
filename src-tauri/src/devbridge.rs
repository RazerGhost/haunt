//! While Haunt is worked on (debug builds only; a release build has none of this), the same calls also answer over
//! HTTP on 127.0.0.1:4778, so the window can run in a browser next to the dev server, with its dev tools and hot
//! reload. The Nuxt dev server forwards `/__haunt` here, so the page calls its own origin:
//! - `POST /__haunt/call/<command>` with the arguments as a JSON body: the answer as JSON, or 400 with `{ "message" }`
//! - `GET /__haunt/events`: the events as server-sent events
//!
//! Only this machine, and only Haunt's own page: the peer must be a loopback address, every request must carry the
//! header `x-haunt: 1` (which a page on another site cannot send without a preflight, and preflights are refused), and
//! an `Origin`, when there is one, must be the dev server's. Written on the standard library, for a handful of requests.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use crate::api::Api;
use crate::events::Events;

pub const PORT: u16 = 4778;
const ORIGINS: [&str; 2] = ["http://127.0.0.1:4777", "http://localhost:4777"];
const MAX_BODY: usize = 1024 * 1024;

pub fn start(api: Arc<Api>, events: Arc<Events>) {
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[haunt] the development bridge cannot listen on 127.0.0.1:{PORT}: {error}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let api = api.clone();
            let events = events.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, &api, &events);
            });
        }
    });
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

fn read(stream: &TcpStream) -> std::io::Result<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = header.split_once(':') {
            headers.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length.min(MAX_BODY)];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, headers, body })
}

fn respond(mut stream: &TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n{body}", body.len())
}

fn serve(stream: TcpStream, api: &Api, events: &Events) -> std::io::Result<()> {
    let local = stream.peer_addr().map(|peer| peer.ip().is_loopback()).unwrap_or(false);
    let request = read(&stream)?;
    let origin_ok = request.header("origin").is_none_or(|origin| ORIGINS.contains(&origin));
    if !local || !origin_ok || request.header("x-haunt") != Some("1") {
        return respond(
            &stream,
            "403 Forbidden",
            r#"{"message":"Only Haunt's own page on this machine can call this."}"#,
        );
    }
    if request.method == "GET" && request.path == "/__haunt/events" {
        let mut out = &stream;
        write!(out, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-store\r\nconnection: keep-alive\r\n\r\n")?;
        out.flush()?;
        let receiver = events.listen();
        loop {
            match receiver.recv_timeout(Duration::from_secs(15)) {
                Ok(news) => write!(out, "data: {news}\n\n")?,
                // A comment now and then, so a closed page is noticed and the stream ends.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => write!(out, ": still here\n\n")?,
                Err(_) => return Ok(()),
            }
            out.flush()?;
        }
    }
    let Some(name) = request.path.strip_prefix("/__haunt/call/").filter(|_| request.method == "POST") else {
        return respond(&stream, "404 Not Found", r#"{"message":"No such call."}"#);
    };
    let args = serde_json::from_slice::<serde_json::Value>(&request.body).unwrap_or(serde_json::Value::Null);
    match crate::api::dispatch(api, name, &args) {
        Ok(answer) => respond(&stream, "200 OK", &answer),
        Err(message) => respond(&stream, "400 Bad Request", &serde_json::json!({ "message": message }).to_string()),
    }
}
