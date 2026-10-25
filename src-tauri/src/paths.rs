//! Paths: one form for every folder Haunt keeps (tabs, recent, trusted), the checks that keep a path from the window
//! inside its repository, and which files a watcher cares about.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// A Windows path without the extended-length prefix (`\\?\C:\…`, `\\?\UNC\server\share\…`) Rust's canonical paths
/// carry: Node cannot start a file given that way, and people do not write paths like that.
pub fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// The one form of a folder: absolute, links resolved, and on Windows in the case the disk has it, so `c:\x` and
/// `C:\X` are the same tab and the same trusted folder. None when it does not exist.
pub fn canonical(path: &str) -> Option<String> {
    let found = fs::canonicalize(path.trim()).ok()?;
    found.is_dir().then(|| plain(found).to_string_lossy().into_owned())
}

/// A path from the window, relative to the repository, as a file inside it; None for anything that could reach out
/// (an absolute path, `..`, a link that leads elsewhere).
pub fn inside(root: &str, relative: &str) -> Option<PathBuf> {
    let candidate = Path::new(relative);
    if relative.is_empty()
        || candidate.components().any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let resolved = plain(fs::canonicalize(Path::new(root).join(candidate)).ok()?);
    resolved.starts_with(root).then_some(resolved)
}

const MANIFESTS: [&str; 8] = [
    "tsconfig.json",
    "package.json",
    "Cargo.toml",
    "composer.json",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
];

/// Whether Haunt can open `path`: a project (a manifest at its root) or any folder in a git work tree. A plain
/// repository opens too; its code lenses say there is nothing to scan.
pub fn openable(path: &Path) -> bool {
    MANIFESTS.iter().any(|file| path.join(file).exists()) || path.ancestors().any(|dir| dir.join(".git").exists())
}

const IGNORED: [&str; 15] = [
    "node_modules",
    "vendor",
    "target",
    ".nuxt",
    ".output",
    ".next",
    ".svelte-kit",
    ".astro",
    ".data",
    ".turbo",
    ".cache",
    "dist",
    "build",
    "coverage",
    ".git",
];

/// A path inside a folder no scan reads (dependencies, builds, caches, git's own).
pub fn ignored(relative: &Path) -> bool {
    relative
        .components()
        .any(|part| matches!(part, Component::Normal(name) if IGNORED.iter().any(|ignored| name == *ignored)))
}

const SOURCES: [&str; 18] = [
    "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "vue", "svelte", "astro", "php", "rs", "lua", "py", "go",
    "json", "toml",
];

/// A file a scan reads (or JSON and TOML, which change what a project is).
pub fn source(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| SOURCES.contains(&ext))
}

/// A file at the root that changes what the project is.
pub fn root_file(name: &str) -> bool {
    matches!(
        name,
        "package.json"
            | "pnpm-workspace.yaml"
            | "haunt.config.json"
            | "Cargo.toml"
            | "composer.json"
            | "go.mod"
            | "pyproject.toml"
    ) || (name.starts_with("tsconfig") && name.ends_with(".json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_paths_inside_the_repository() {
        let root = plain(fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()).to_string_lossy().into_owned();
        assert!(inside(&root, "Cargo.toml").is_some());
        assert!(inside(&root, "src/lib.rs").is_some());
        assert!(inside(&root, "../package.json").is_none());
        assert!(inside(&root, "src/../../package.json").is_none());
        assert!(inside(&root, "").is_none());
        assert!(inside(&root, if cfg!(windows) { r"C:\Windows" } else { "/etc" }).is_none());
        assert!(inside(&root, "no-such-file").is_none());
    }

    #[test]
    fn one_form_for_a_folder() {
        let root = env!("CARGO_MANIFEST_DIR");
        let found = canonical(root).unwrap();
        assert!(!found.starts_with(r"\\?\"));
        if cfg!(windows) {
            assert_eq!(canonical(&root.to_uppercase()), Some(found.clone()));
            assert_eq!(canonical(&root.to_lowercase()), Some(found));
        }
        assert_eq!(canonical("no-such-folder-anywhere"), None);
    }

    #[test]
    fn tells_which_files_matter() {
        assert!(ignored(Path::new("a/node_modules/b.ts")));
        assert!(!ignored(Path::new("src/node_modules_like/b.ts")));
        assert!(source(Path::new("a/b.svelte")));
        assert!(source(Path::new("a/b.py")) && source(Path::new("a/b.go")));
        assert!(!source(Path::new("a/b.md")));
        assert!(root_file("go.mod") && root_file("pyproject.toml"));
        assert!(root_file("tsconfig.app.json"));
        assert!(root_file("haunt.config.json"));
        assert!(!root_file("README.md"));
    }
}
