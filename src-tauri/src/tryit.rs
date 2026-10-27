//! Try it: a request to a dev server on this machine, from a route in the Routes view, and requests kept as `.http`
//! files in the repository (the format REST Client and other tools read), so they are versioned with the code.
//!
//! Plain HTTP/1.1 over the standard library's TCP, to local addresses only (localhost, 127.0.0.1, [::1], *.localhost):
//! nothing leaves the machine, so no TLS and no HTTP library. A general API client (sign-in flows, proxies,
//! collections) is left to the tools made for it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

/// The most of a response body read.
const MOST_BODY: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
/// Where saved requests go in a repository.
pub const FOLDER: &str = "http";

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    /// Its name in its file (`### name`); empty when it has none.
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub truncated: bool,
    pub ms: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HttpFile {
    /// Relative to the repository.
    pub path: String,
    /// Its `@name = value` variables.
    pub variables: Vec<(String, String)>,
    pub requests: Vec<HttpRequest>,
}

/// The host, port and path of a local `http://` address; anything else is refused.
fn local(url: &str) -> Result<(String, u16, String), String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        said!("tryit.localOnly", "Only http:// addresses on this machine: a dev server, such as http://localhost:3000.")
    })?;
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    if authority.contains('@') {
        return Err(said!("tryit.noCredentials", "Leave the user name and password out of the address."));
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) =
            inner.split_once(']').ok_or_else(|| said!("tryit.incomplete", "That address is not complete."))?;
        (host.to_owned(), after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_owned(), Some(port)),
            None => (authority.to_owned(), None),
        }
    };
    let port = match port {
        Some(port) => {
            port.parse::<u16>().map_err(|_| said!("tryit.notPort", "\"{port}\" is not a port.", port = port))?
        }
        None => 80,
    };
    let host_lower = host.to_ascii_lowercase();
    let allowed =
        host_lower == "localhost" || host_lower.ends_with(".localhost") || host == "127.0.0.1" || host == "::1";
    if !allowed {
        return Err(said!(
            "tryit.notLocal",
            "{host} is not this machine: Try it only calls dev servers on localhost.",
            host = host
        ));
    }
    Ok((host, port, path.to_owned()))
}

fn checked_header(name: &str, value: &str) -> Result<(), String> {
    let bad = |text: &str| text.contains(['\r', '\n', '\0']);
    if name.is_empty() || bad(name) || bad(value) || name.contains(':') {
        return Err(said!("tryit.badHeader", "\"{name}\" is not a header Haunt can send.", name = name));
    }
    Ok(())
}

/// A chunked body, joined.
fn unchunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(end) = data.windows(2).position(|pair| pair == b"\r\n") {
        let size_text = String::from_utf8_lossy(&data[..end]);
        let Ok(size) = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16) else { break };
        data = &data[end + 2..];
        if size == 0 || data.len() < size {
            out.extend_from_slice(&data[..size.min(data.len())]);
            break;
        }
        out.extend_from_slice(&data[..size]);
        data = data.get(size + 2..).unwrap_or_default();
    }
    out
}

/// Sends `request` to a dev server on this machine and reads the answer (at most 2 MB of body, 30 seconds).
pub fn send(request: &HttpRequest) -> Result<HttpResponse, String> {
    let method = request.method.trim().to_ascii_uppercase();
    if method.is_empty() || !method.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(said!("tryit.notMethod", "\"{method}\" is not an HTTP method.", method = request.method));
    }
    let (host, port, path) = local(request.url.trim())?;
    for (name, value) in &request.headers {
        checked_header(name, value)?;
    }
    let started = Instant::now();
    // localhost is both ::1 and 127.0.0.1, and a dev server may listen on only one of them: each is tried, the one that
    // answered last time first (a refused address can take seconds to say so).
    static ANSWERED: OnceLock<Mutex<HashMap<(String, u16), SocketAddr>>> = OnceLock::new();
    let answered = ANSWERED.get_or_init(Default::default);
    let mut addresses: Vec<SocketAddr> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("{host}: {error}"))?
        .filter(|address| address.ip().is_loopback())
        .collect();
    if addresses.is_empty() {
        return Err(said!("tryit.notThisMachine", "{host} does not lead to this machine.", host = host));
    }
    let key = (host.to_ascii_lowercase(), port);
    if let Some(last) = answered.lock().unwrap().get(&key) {
        addresses.sort_by_key(|address| address != last);
    }
    let mut refused = String::new();
    let mut connected = None;
    for address in &addresses {
        match TcpStream::connect_timeout(address, Duration::from_secs(5)) {
            Ok(stream) => {
                answered.lock().unwrap().insert(key, *address);
                connected = Some(stream);
                break;
            }
            Err(error) => refused = error.to_string(),
        }
    }
    let mut stream = connected.ok_or_else(|| {
        said!(
            "tryit.nothingAnswers",
            "Nothing answers at {url}: is the dev server running? ({refused})",
            url = request.url.trim(),
            refused = refused
        )
    })?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();
    let host_header = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let mut head =
        format!("{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nUser-Agent: Haunt\r\n");
    let named = |wanted: &str| request.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case(wanted));
    if !named("accept") {
        head.push_str("Accept: */*\r\n");
    }
    for (name, value) in &request.headers {
        if !["host", "connection", "content-length"].contains(&name.to_ascii_lowercase().as_str()) {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    if !request.body.is_empty() || ["POST", "PUT", "PATCH"].contains(&method.as_str()) {
        head.push_str(&format!("Content-Length: {}\r\n", request.body.len()));
    }
    head.push_str("\r\n");
    head.push_str(&request.body);
    stream.write_all(head.as_bytes()).map_err(|error| error.to_string())?;

    let mut raw = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    let mut truncated = false;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                raw.extend_from_slice(&buffer[..count]);
                if raw.len() > MOST_BODY + 64 * 1024 {
                    truncated = true;
                    break;
                }
            }
            Err(error) if raw.is_empty() => {
                return Err(said!("tryit.noAnswer", "The dev server did not answer: {error}", error = error))
            }
            Err(_) => break,
        }
    }
    let ms = started.elapsed().as_millis() as u32;
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| said!("tryit.notHttp", "The answer is not HTTP."))?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next();
    let status = parts
        .next()
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| said!("tryit.notHttp", "The answer is not HTTP."))?;
    let reason = parts.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let mut body = raw[split + 4..].to_vec();
    if headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding") && value.to_ascii_lowercase().contains("chunked")
    }) {
        body = unchunk(&body);
    }
    if body.len() > MOST_BODY {
        body.truncate(MOST_BODY);
        truncated = true;
    }
    Ok(HttpResponse { status, reason, headers, body: String::from_utf8_lossy(&body).into_owned(), truncated, ms })
}

/* ---------- .http files ---------- */

/// Reads a `.http` file: `@name = value` variables, and requests separated by `###` lines (the text after `###` names
/// the next request), each a request line, headers, a blank line and a body.
pub fn parse(text: &str) -> (Vec<(String, String)>, Vec<HttpRequest>) {
    let mut variables = Vec::new();
    let mut requests = Vec::new();
    // The request being read: its name until its request line, then it, and whether its body has begun.
    let mut name = String::new();
    let mut current: Option<HttpRequest> = None;
    let mut in_body = false;
    let mut body: Vec<&str> = Vec::new();
    let finish = |current: &mut Option<HttpRequest>, body: &mut Vec<&str>, requests: &mut Vec<HttpRequest>| {
        if let Some(mut request) = current.take() {
            request.body = body.join("\n").trim_end().to_owned();
            requests.push(request);
        }
        body.clear();
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("###") {
            finish(&mut current, &mut body, &mut requests);
            name = rest.trim().to_owned();
            in_body = false;
            continue;
        }
        let Some(request) = current.as_mut() else {
            if let Some(rest) = trimmed.strip_prefix("# @name").or_else(|| trimmed.strip_prefix("// @name")) {
                name = rest.trim().to_owned();
            } else if let Some((key, value)) = trimmed.strip_prefix('@').and_then(|rest| rest.split_once('=')) {
                variables.push((key.trim().to_owned(), value.trim().to_owned()));
            } else if looks_like_request(trimmed) {
                let mut words = trimmed.split_whitespace();
                let method = words.next().unwrap_or_default().to_owned();
                let url = words.next().unwrap_or_default().to_owned();
                current = Some(HttpRequest {
                    name: std::mem::take(&mut name),
                    method,
                    url,
                    headers: Vec::new(),
                    body: String::new(),
                });
            }
            continue;
        };
        if in_body {
            body.push(line);
        } else if trimmed.is_empty() {
            in_body = true;
        } else if let Some((key, value)) = line.split_once(':') {
            request.headers.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    finish(&mut current, &mut body, &mut requests);
    (variables, requests)
}

fn looks_like_request(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or_default();
    let url = words.next().unwrap_or_default();
    !method.is_empty()
        && method.chars().all(|c| c.is_ascii_uppercase())
        && (url.starts_with("http") || url.starts_with("{{") || url.starts_with('/'))
}

/// Writes requests as a `.http` file's text.
pub fn write(variables: &[(String, String)], requests: &[HttpRequest]) -> String {
    let mut out = String::new();
    for (name, value) in variables {
        out.push_str(&format!("@{name} = {value}\n"));
    }
    for request in requests {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("### {}\n{} {}\n", request.name, request.method, request.url));
        for (name, value) in &request.headers {
            out.push_str(&format!("{name}: {value}\n"));
        }
        if !request.body.is_empty() {
            out.push_str(&format!("\n{}\n", request.body));
        }
    }
    out
}

/// The `.http` files at the top of the repository and in its `http/` folder.
pub fn files(root: &str) -> Vec<HttpFile> {
    let mut out = Vec::new();
    for dir in ["", FOLDER] {
        let Ok(entries) = std::fs::read_dir(Path::new(root).join(dir)) else { continue };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".http"))
            .collect();
        names.sort();
        for name in names {
            let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
            let Ok(text) = std::fs::read_to_string(Path::new(root).join(&path)) else { continue };
            let (variables, requests) = parse(&text);
            out.push(HttpFile { path, variables, requests });
        }
    }
    out
}

/// Saves a request in `http/<name>.http`, replacing the request of the same name there, with `variables` at the top.
pub fn save(root: &str, file: &str, variables: &[(String, String)], request: &HttpRequest) -> Result<String, String> {
    let bad =
        file.is_empty() || !file.ends_with(".http") || file.contains(['/', '\\', ':', '\0']) || file.starts_with('.');
    if bad {
        return Err(said!("tryit.badFile", "\"{file}\" is not a file name for requests (name.http).", file = file));
    }
    let folder = PathBuf::from(root).join(FOLDER);
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(file);
    let (mut kept_variables, mut requests) =
        std::fs::read_to_string(&path).map(|text| parse(&text)).unwrap_or_default();
    for (name, value) in variables {
        match kept_variables.iter_mut().find(|(kept, _)| kept == name) {
            Some(slot) => slot.1 = value.clone(),
            None => kept_variables.push((name.clone(), value.clone())),
        }
    }
    match requests.iter_mut().find(|kept| kept.name == request.name) {
        Some(slot) => *slot = request.clone(),
        None => requests.push(request.clone()),
    }
    std::fs::write(&path, write(&kept_variables, &requests)).map_err(|error| error.to_string())?;
    Ok(format!("{FOLDER}/{file}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn only_local_addresses() {
        assert!(local("http://localhost:3000/api").is_ok());
        assert_eq!(local("http://127.0.0.1/x").unwrap(), ("127.0.0.1".into(), 80, "/x".into()));
        assert_eq!(local("http://[::1]:8080").unwrap(), ("::1".into(), 8080, "/".into()));
        assert!(local("http://app.localhost:5173/").is_ok());
        assert!(local("https://localhost:3000").is_err());
        assert!(local("http://example.com/").is_err());
        assert!(local("http://localhost.example.com/").is_err());
        assert!(local("http://user:pw@localhost/").is_err());
    }

    #[test]
    fn sends_a_request_and_reads_a_chunked_answer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = vec![0u8; 4096];
            let count = stream.read(&mut seen).unwrap();
            let asked = String::from_utf8_lossy(&seen[..count]).into_owned();
            stream
                .write_all(b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"id\":1\r\n1\r\n}\r\n0\r\n\r\n")
                .unwrap();
            asked
        });
        let response = send(&HttpRequest {
            name: String::new(),
            method: "post".into(),
            url: format!("http://localhost:{port}/api/entries"),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: "{\"title\":\"A\"}".into(),
        })
        .unwrap();
        let asked = server.join().unwrap();
        assert!(asked.starts_with("POST /api/entries HTTP/1.1\r\n"));
        assert!(asked.contains("Content-Length: 13\r\n") && asked.ends_with("{\"title\":\"A\"}"));
        assert_eq!((response.status, response.reason.as_str(), response.body.as_str()), (201, "Created", "{\"id\":1}"));
        assert!(send(&HttpRequest {
            name: String::new(),
            method: "GET".into(),
            url: format!("http://127.0.0.1:{port}/"),
            headers: vec![("X\r\nEvil".into(), "1".into())],
            body: String::new()
        })
        .is_err());
    }

    #[test]
    fn reads_and_writes_http_files() {
        let text = "@baseUrl = http://localhost:3000\n\n### List entries\nGET {{baseUrl}}/api/entries\nAccept: application/json\n\n### Create\nPOST {{baseUrl}}/api/entries\nContent-Type: application/json\n\n{\n  \"title\": \"A\"\n}\n";
        let (variables, requests) = parse(text);
        assert_eq!(variables, [("baseUrl".to_owned(), "http://localhost:3000".to_owned())]);
        assert_eq!(requests.len(), 2);
        assert_eq!(
            (requests[0].name.as_str(), requests[0].method.as_str(), requests[0].url.as_str()),
            ("List entries", "GET", "{{baseUrl}}/api/entries")
        );
        assert_eq!(requests[0].headers, [("Accept".to_owned(), "application/json".to_owned())]);
        assert_eq!(requests[1].body, "{\n  \"title\": \"A\"\n}");
        assert_eq!(parse(&write(&variables, &requests)), (variables.clone(), requests.clone()), "written as read");

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let saved = save(&root, "entries.http", &variables, &requests[0]).unwrap();
        assert_eq!(saved, "http/entries.http");
        let mut changed = requests[0].clone();
        changed.headers.clear();
        save(&root, "entries.http", &variables, &changed).unwrap();
        save(&root, "entries.http", &variables, &requests[1]).unwrap();
        let found = files(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].requests.len(), 2, "the request of the same name is replaced");
        assert!(found[0].requests[0].headers.is_empty());
        assert!(save(&root, "../out.http", &variables, &requests[0]).is_err());
    }
}
