//! The languages Haunt reads, by file extension, as the sidecar's `languages.ts` has them. The TypeScript family goes
//! through the TypeScript compiler in the sidecar (TypeScript, JavaScript, and Vue, Svelte and Astro files); PHP, Rust,
//! Lua, Python and Go through tree-sitter here, with calls matched by name. `shared/languages.cases.json` holds paths both copies are
//! tested against, so they cannot drift apart.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Typescript,
    Php,
    Rust,
    Lua,
    Python,
    Go,
}

/// Larger files are generated or vendored, not written.
pub const MAX_SOURCE_BYTES: u64 = 512 * 1024;

fn ends_with_any(path: &str, endings: &[&str]) -> bool {
    endings.iter().any(|ending| path.ends_with(ending))
}

/// `.d.ts`, `.d.mts`, `.d.cts`: declarations, not code.
fn declaration(path: &str) -> bool {
    ends_with_any(path, &[".d.ts", ".d.mts", ".d.cts"])
}

/// Built or bundled JavaScript (`app.min.js`, `chunk-1a2b3c4d.js`): not anyone's source.
fn built(path: &str) -> bool {
    let Some(stem) = ["js", "mjs", "cjs"].iter().find_map(|ext| path.strip_suffix(&format!(".{ext}"))) else {
        return false;
    };
    if ["min", "bundle", "umd", "prod", "production"].iter().any(|word| stem.ends_with(&format!(".{word}"))) {
        return true;
    }
    // A content hash of at least eight hex digits after a dot or a dash.
    let hex = stem.chars().rev().take_while(|c| c.is_ascii_digit() || ('a'..='f').contains(c)).count();
    hex >= 8 && matches!(stem[..stem.len() - hex].chars().last(), Some('.') | Some('-'))
}

const TYPESCRIPT: [&str; 15] = [
    ".ts", ".tsx", ".mts", ".cts", ".mtsx", ".ctsx", ".js", ".jsx", ".mjs", ".cjs", ".mjsx", ".cjsx", ".vue",
    ".svelte", ".astro",
];

pub fn language_of(path: &str) -> Option<Language> {
    if declaration(path) || built(path) {
        return None;
    }
    if ends_with_any(path, &TYPESCRIPT) {
        return Some(Language::Typescript);
    }
    if path.ends_with(".php") {
        return Some(Language::Php);
    }
    if path.ends_with(".rs") {
        return Some(Language::Rust);
    }
    if path.ends_with(".lua") {
        return Some(Language::Lua);
    }
    if path.ends_with(".py") {
        return Some(Language::Python);
    }
    if path.ends_with(".go") {
        return Some(Language::Go);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases the sidecar's tests check too (shared/languages.cases.json).
    #[test]
    fn agrees_with_the_sidecar_on_the_shared_cases() {
        let cases: Vec<(String, Option<Language>)> =
            serde_json::from_str(include_str!("../../../shared/languages.cases.json")).unwrap();
        for (path, expected) in cases {
            assert_eq!(language_of(&path), expected, "{path}");
        }
    }
}
