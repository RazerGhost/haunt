//! Pages of the commit list, through `git log`: git walks the history in topological order (children before parents,
//! as the graph needs) and simplifies it for a path, rewriting each commit's parents to the earlier commits that touch
//! it too, which keeps the graph connected. Names from the window go after `--end-of-options`.
//!
//! git works the whole order out before it gives the first commit, and a search (`-S`) reads every commit, so asking
//! again for each page would redo all of it. The `git log` of the last query keeps running instead, and the next page
//! is read on from where the last one stopped; the first page always starts afresh (new commits, moved branches), as
//! does any other query. A reader nobody asked for a minute is stopped.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::cli::{revision, stream, Lines};
use super::types::{Decoration, DecorationKind, LogCommit, LogPage, LogQuery};

/// The running `git log` of the last query, and how far it was read.
struct Reader {
    /// The repository and the query's arguments.
    key: String,
    /// Commits read so far (from the start of the list).
    next: u32,
    lines: Lines,
    /// A line read ahead, to tell whether there is more.
    ahead: Option<String>,
    used: Instant,
}

/// How long a reader is kept without a page asked of it.
const IDLE: Duration = Duration::from_secs(60);

fn reader() -> &'static Mutex<Option<Reader>> {
    static READER: OnceLock<Mutex<Option<Reader>>> = OnceLock::new();
    READER.get_or_init(|| {
        // Stops a reader left idle: it keeps git, and on Windows its pack files, open.
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(20));
            let mut kept = reader().lock().unwrap();
            if kept.as_ref().is_some_and(|reader| reader.used.elapsed() > IDLE) {
                *kept = None;
            }
        });
        Mutex::new(None)
    })
}

const FIELD: char = '\x1f';

/// %D with --decorate=full: "HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1".
pub fn decorations(text: &str) -> Vec<Decoration> {
    let mut out = Vec::new();
    for item in text.split(", ").filter(|item| !item.is_empty()) {
        let item = match item.strip_prefix("HEAD -> ") {
            Some(rest) => {
                out.push(Decoration { kind: DecorationKind::Head, name: "HEAD".into() });
                rest
            }
            None => item,
        };
        let found = if item == "HEAD" {
            Some((DecorationKind::Head, "HEAD"))
        } else if let Some(name) = item.strip_prefix("tag: refs/tags/") {
            Some((DecorationKind::Tag, name))
        } else if let Some(name) = item.strip_prefix("refs/heads/") {
            Some((DecorationKind::Branch, name))
        } else {
            item.strip_prefix("refs/remotes/")
                .filter(|name| !name.ends_with("/HEAD"))
                .map(|name| (DecorationKind::Remote, name))
        };
        if let Some((kind, name)) = found {
            out.push(Decoration { kind, name: name.into() });
        }
    }
    out
}

fn commit(line: &str) -> LogCommit {
    let mut parts = line.split(FIELD);
    let mut next = || parts.next().unwrap_or_default().to_owned();
    let (sha, parents, author, email, time, subject, refs) = (next(), next(), next(), next(), next(), next(), next());
    LogCommit {
        sha,
        parents: parents.split(' ').filter(|parent| !parent.is_empty()).map(str::to_owned).collect(),
        author,
        email,
        time: time.parse().unwrap_or(0),
        subject,
        refs: decorations(&refs),
        signed: false,
    }
}

/// A page of the commit list, newest first with children before their parents.
pub fn log(root: &str, query: &LogQuery) -> Result<LogPage, String> {
    let limit = query.limit.unwrap_or(300).clamp(1, 2000);
    let skip = query.skip.unwrap_or(0);
    let format = ["%H", "%P", "%an", "%ae", "%at", "%s", "%D"].join("%x1f");
    let mut args: Vec<String> = vec![
        "log".into(),
        if query.by_date == Some(true) { "--date-order" } else { "--topo-order" }.into(),
        "--decorate=full".into(),
        format!("--format={format}"),
    ];
    if let Some(text) = query.changed.as_deref().filter(|text| !text.is_empty()) {
        args.push(format!("-S{text}"));
    }
    match query.merges {
        Some(super::types::MergeFilter::Only) => args.push("--merges".into()),
        Some(super::types::MergeFilter::None) => args.push("--no-merges".into()),
        None => {}
    }
    let author = query.author.as_deref().filter(|text| !text.is_empty());
    let message = query.message.as_deref().filter(|text| !text.is_empty());
    if author.is_some() || message.is_some() {
        args.extend(["--regexp-ignore-case".into(), "--fixed-strings".into()]);
    }
    if let Some(author) = author {
        args.push(format!("--author={author}"));
    }
    if let Some(message) = message {
        args.push(format!("--grep={message}"));
    }
    let path = query.path.as_deref().filter(|path| !path.is_empty());
    if path.is_some() {
        args.push("--parents".into());
    }
    let from: Vec<&str> = query.from.iter().flatten().map(String::as_str).filter(|name| !name.is_empty()).collect();
    // Names from the window after --end-of-options, so none can pass for an option; the "everything" flags before it.
    if from.is_empty() {
        args.extend([
            "--branches".into(),
            "--remotes".into(),
            "--tags".into(),
            "--end-of-options".into(),
            "HEAD".into(),
        ]);
    } else {
        args.push("--end-of-options".into());
        for name in from {
            args.push(revision(name)?.to_owned());
        }
    }
    args.push("--".into());
    if let Some(path) = path {
        args.push(path.to_owned());
    }
    let key = format!("{root}\0{}", args.join("\0"));
    let mut kept = reader().lock().unwrap();
    let going_on = skip > 0 && kept.as_ref().is_some_and(|reader| reader.key == key && reader.next == skip);
    if !going_on {
        // A new reader, from this page on.
        *kept = None;
        args.insert(1, format!("--skip={skip}"));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        *kept = Some(Reader { key, next: skip, lines: stream(root, &refs)?, ahead: None, used: Instant::now() });
    }
    let reader = kept.as_mut().expect("kept");
    reader.used = Instant::now();
    let mut commits: Vec<LogCommit> = Vec::new();
    // The page, and one line more to know whether there is more.
    let mut more = false;
    let mut failed = None;
    loop {
        let line = match reader.ahead.take() {
            Some(line) => Some(Ok(line)),
            None => reader.lines.next_line(),
        };
        match line {
            None => break,
            Some(Ok(line)) if line.is_empty() => continue,
            Some(Ok(line)) if commits.len() == limit as usize => {
                reader.ahead = Some(line);
                more = true;
                break;
            }
            Some(Ok(line)) => commits.push(commit(&line)),
            Some(Err(error)) => {
                failed = Some(error);
                break;
            }
        }
    }
    if let Some(error) = failed {
        *kept = None;
        // A repository without a commit yet has no HEAD to list.
        let empty = query.from.as_ref().is_none_or(Vec::is_empty)
            && (error.contains("does not have any commits")
                || error.contains("bad default revision")
                || error.contains("unknown revision"));
        return if empty {
            Ok(LogPage { commits: Vec::new(), more: false })
        } else {
            Err(error.lines().next().unwrap_or(&error).to_owned())
        };
    }
    let signed = super::objects::signed(root, commits.iter().map(|commit| commit.sha.as_str()));
    for (commit, signed) in commits.iter_mut().zip(signed) {
        commit.signed = signed;
    }
    reader.next += commits.len() as u32;
    if !more {
        *kept = None;
    }
    Ok(LogPage { commits, more })
}
