//! A repository's health: how much room its history takes, the files that take the most of it (in every version, still
//! there or long deleted), and the files now in it that belong in Git LFS. Read only; the window adds the old branches
//! from the refs it has.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use specta::Type;

use super::cli::{git, pipe, top};

/// The most files each list holds.
const LISTED: usize = 15;
/// A file this big belongs in Git LFS whatever it is.
const BIG: u64 = 1024 * 1024;
/// A file of a kind that does not diff (images, media, archives…) this big belongs there too.
const BIG_BINARY: u64 = 256 * 1024;
const BINARY: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "tif", "tiff", "ico", "psd", "ai", "sketch", "fig", "mp4",
    "mov", "avi", "mkv", "webm", "mp3", "wav", "flac", "ogg", "zip", "gz", "tgz", "7z", "rar", "jar", "exe", "dll",
    "so", "dylib", "bin", "iso", "dmg", "pdf", "woff", "woff2", "ttf", "otf", "glb", "fbx", "blend", "sqlite", "db",
];

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFile {
    pub path: String,
    /// Every version of it in the history together, and the biggest one.
    pub bytes: u64,
    pub largest: u64,
    pub versions: u32,
    /// Whether the last commit still has it.
    pub in_head: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LfsCandidate {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// What the history takes on disk, packed and loose.
    pub bytes: u64,
    pub loose_objects: u32,
    pub packs: u32,
    /// The files that take the most room across the history.
    pub largest: Vec<HistoryFile>,
    /// Files in the last commit that belong in Git LFS and are not in it.
    pub lfs: Vec<LfsCandidate>,
    /// Whether the repository uses Git LFS for anything.
    pub uses_lfs: bool,
}

/// Paths are the repository's own, from its top folder, whatever folder was opened.
pub fn health(root: &str) -> Result<Health, String> {
    let (top, _) = top(root)?;
    let root = top.as_str();
    let counts: HashMap<String, u64> = git(root, &["count-objects", "-v"])?
        .lines()
        .filter_map(|line| line.split_once(": "))
        .map(|(key, value)| (key.to_owned(), value.trim().parse().unwrap_or(0)))
        .collect();
    let kib = |key: &str| counts.get(key).copied().unwrap_or(0) * 1024;

    // Every version of every file in the history, with its size and the path it was first seen at: rev-list names each
    // object once, and cat-file says what it is and how big, read as it comes rather than all at once.
    let mut by_path: HashMap<String, (u64, u64, u32)> = HashMap::new();
    let mut objects = pipe(
        root,
        &["rev-list", "--objects", "--all"],
        &["cat-file", "--batch-check=%(objecttype) %(objectsize) %(rest)"],
    )?;
    while let Some(line) = objects.next_line() {
        let line = line?;
        let mut parts = line.splitn(3, ' ');
        if parts.next() != Some("blob") {
            continue;
        }
        let (Some(Ok(size)), Some(path)) = (parts.next().map(str::parse::<u64>), parts.next()) else { continue };
        if path.is_empty() {
            continue;
        }
        let entry = by_path.entry(path.to_owned()).or_default();
        entry.0 += size;
        entry.1 = entry.1.max(size);
        entry.2 += 1;
    }

    // The last commit's files, with their sizes.
    let mut head: Vec<(String, u64)> = Vec::new();
    if let Ok(listing) = git(root, &["ls-tree", "-r", "-l", "-z", "--full-tree", "HEAD"]) {
        for entry in listing.split('\0') {
            let Some((meta, path)) = entry.split_once('\t') else { continue };
            let fields: Vec<&str> = meta.split_whitespace().collect();
            if fields.get(1) == Some(&"blob") {
                head.push((path.to_owned(), fields.get(3).and_then(|size| size.parse().ok()).unwrap_or(0)));
            }
        }
    }
    let in_head: HashSet<&str> = head.iter().map(|(path, _)| path.as_str()).collect();

    let mut largest: Vec<HistoryFile> = by_path
        .into_iter()
        .map(|(path, (bytes, biggest, versions))| HistoryFile {
            in_head: in_head.contains(path.as_str()),
            path,
            bytes,
            largest: biggest,
            versions,
        })
        .collect();
    largest.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    largest.truncate(LISTED);

    let binary = |path: &str| {
        let extension = path.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
        BINARY.contains(&extension.as_str())
    };
    let mut big: Vec<(String, u64)> =
        head.into_iter().filter(|(path, bytes)| *bytes >= BIG || (*bytes >= BIG_BINARY && binary(path))).collect();
    big.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // Those Git LFS keeps already are fine: they are pointers in the history.
    let mut lfs_kept = HashSet::new();
    if !big.is_empty() {
        let mut args = vec!["check-attr", "-z", "filter", "--"];
        args.extend(big.iter().map(|(path, _)| path.as_str()));
        let out = git(root, &args).unwrap_or_default();
        let fields: Vec<&str> = out.split('\0').collect();
        for chunk in fields.chunks(3) {
            if let [path, _, value] = chunk {
                if *value == "lfs" {
                    lfs_kept.insert((*path).to_owned());
                }
            }
        }
    }
    let uses_lfs = !lfs_kept.is_empty()
        || std::fs::read_to_string(std::path::Path::new(root).join(".gitattributes"))
            .is_ok_and(|text| text.contains("filter=lfs"));
    let lfs = big
        .into_iter()
        .filter(|(path, _)| !lfs_kept.contains(path))
        .take(LISTED)
        .map(|(path, bytes)| LfsCandidate { path, bytes })
        .collect();

    Ok(Health {
        bytes: kib("size-pack") + kib("size"),
        loose_objects: counts.get("count").copied().unwrap_or(0) as u32,
        packs: counts.get("packs").copied().unwrap_or(0) as u32,
        largest,
        lfs,
        uses_lfs,
    })
}
