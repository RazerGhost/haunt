//! The icon sets a theme may use besides Lucide (which is bundled): downloaded on request by the window, from the
//! Iconify API and only the icons Haunt names, and kept here, in the app's config folder, one JSON file per set, so they
//! work offline from then on. What is kept is checked first: an Iconify collection of the set's own prefix, whose icons
//! are SVG drawings with nothing that could run (no scripts, event attributes, links or embedded documents), rewritten
//! with only the fields the window draws from.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// Each set a theme may pick, and its Iconify prefix.
const SETS: [(&str, &str); 7] = [
    ("bootstrap", "bi"),
    ("phosphor", "ph"),
    ("tabler", "tabler"),
    ("remix", "ri"),
    ("material", "material-symbols"),
    ("heroicons", "heroicons"),
    ("iconoir", "iconoir"),
];
/// The most a set's file may hold: the ~110 icons Haunt names are well under it.
const MOST: usize = 1_000_000;

fn prefix_of(set: &str) -> Result<&'static str, String> {
    SETS.iter()
        .find(|(name, _)| *name == set)
        .map(|(_, prefix)| *prefix)
        .ok_or_else(|| said!("icons.unknownSet", "{set} is not an icon set Haunt knows.", set = set))
}

fn file(dir: &Path, set: &str) -> PathBuf {
    dir.join("icons").join(format!("{set}.json"))
}

/// The sets downloaded so far.
pub fn downloaded(dir: &Path) -> Vec<String> {
    SETS.iter().filter(|(set, _)| file(dir, set).is_file()).map(|(set, _)| (*set).to_owned()).collect()
}

/// A downloaded set's icons, as kept.
pub fn read(dir: &Path, set: &str) -> Result<String, String> {
    prefix_of(set)?;
    std::fs::read_to_string(file(dir, set))
        .map_err(|_| said!("icons.notDownloaded", "The {set} icons are not downloaded.", set = set))
}

/// Checks and keeps a set the window downloaded (`text`, the Iconify API's answer).
pub fn save(dir: &Path, set: &str, text: &str) -> Result<(), String> {
    let kept = checked(set, text)?;
    let path = file(dir, set);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
    std::fs::write(&path, kept)
        .map_err(|error| said!("icons.notKept", "The {set} icons could not be kept: {error}", set = set, error = error))
}

pub fn delete(dir: &Path, set: &str) -> Result<(), String> {
    prefix_of(set)?;
    match std::fs::remove_file(file(dir, set)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// An SVG drawing with nothing that could run or reach out.
fn safe(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    let forbidden = ["<script", "javascript:", "<foreignobject", "<iframe", "<object", "<embed", "<a ", "<image"];
    if forbidden.iter().any(|word| lower.contains(word)) {
        return false;
    }
    // References only within the drawing itself: `url(#mask)`, `href="#shape"`.
    let local = |marker: &str, quotes: bool| {
        lower.match_indices(marker).all(|(at, _)| {
            let rest = lower[at + marker.len()..].trim_start();
            let rest = if quotes {
                rest.trim_start_matches(['=', ' ', '"', '\''])
            } else {
                rest.trim_start_matches(['"', '\''])
            };
            rest.starts_with('#')
        })
    };
    if !local("url(", false) || !local("href", true) {
        return false;
    }
    // Event attributes: ` on…=` anywhere.
    let bytes = lower.as_bytes();
    !bytes.windows(3).enumerate().any(|(at, window)| {
        window[1..] == *b"on" && window[0].is_ascii_whitespace() && {
            let rest = &lower[at + 3..];
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            !name.is_empty() && rest[name.len()..].trim_start().starts_with('=')
        }
    })
}

fn checked(set: &str, text: &str) -> Result<String, String> {
    let prefix = prefix_of(set)?;
    if text.len() > MOST {
        return Err(said!("icons.tooLarge", "The downloaded icons are too large."));
    }
    let value: Value =
        serde_json::from_str(text).map_err(|_| said!("icons.notASet", "The downloaded icons are not an icon set."))?;
    if value["prefix"].as_str() != Some(prefix) {
        return Err(said!("icons.otherSet", "The downloaded icons are not {set}'s.", set = set));
    }
    let number = |value: &Value| value.as_f64().filter(|number| (0.0..=10_000.0).contains(number));
    let mut icons = Map::new();
    for (name, icon) in
        value["icons"].as_object().ok_or_else(|| said!("icons.noIcons", "The downloaded icons have no icons."))?
    {
        let body = icon["body"]
            .as_str()
            .ok_or_else(|| said!("icons.noDrawing", "The icon {name} has no drawing.", name = name))?;
        if !safe(body) {
            return Err(said!(
                "icons.moreThanDrawing",
                "The icon {name} holds more than a drawing; it was not kept.",
                name = name
            ));
        }
        let mut kept = Map::new();
        kept.insert("body".into(), body.into());
        for key in ["width", "height", "left", "top"] {
            if let Some(number) = number(&icon[key]) {
                kept.insert(key.into(), number.into());
            }
        }
        icons.insert(name.clone(), Value::Object(kept));
    }
    let mut aliases = Map::new();
    for (name, alias) in value["aliases"].as_object().into_iter().flatten() {
        if let Some(parent) = alias["parent"].as_str().filter(|parent| icons.contains_key(*parent)) {
            aliases.insert(name.clone(), json!({ "parent": parent }));
        }
    }
    let mut out = json!({ "prefix": prefix, "icons": icons, "aliases": aliases });
    for key in ["width", "height"] {
        if let Some(number) = number(&value[key]) {
            out[key] = number.into();
        }
    }
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_checked_set_and_refuses_one_that_could_run_something() {
        let dir = tempfile::tempdir().unwrap();
        let answer = r#"{ "prefix": "ph", "width": 256, "height": 256, "lastModified": 1,
            "icons": { "check": { "body": "<path fill=\"currentColor\" d=\"m1 2\"/>", "extra": "dropped" } },
            "aliases": { "tick": { "parent": "check" }, "lost": { "parent": "nowhere" } } }"#;
        save(dir.path(), "phosphor", answer).unwrap();
        assert_eq!(downloaded(dir.path()), ["phosphor"]);
        let kept: Value = serde_json::from_str(&read(dir.path(), "phosphor").unwrap()).unwrap();
        assert_eq!(kept["icons"]["check"], json!({ "body": "<path fill=\"currentColor\" d=\"m1 2\"/>" }));
        assert_eq!(kept["aliases"], json!({ "tick": { "parent": "check" } }));
        assert_eq!(kept["width"], 256.0);

        for body in [
            "<script>alert(1)</script>",
            "<path onload=\"alert(1)\"/>",
            "<path ONclick = 'x'/>",
            "<image href=\"https://example.com/x.png\"/>",
            "<path style=\"fill:url(https://example.com)\"/>",
        ] {
            let bad = json!({ "prefix": "ph", "icons": { "x": { "body": body } } }).to_string();
            assert!(save(dir.path(), "phosphor", &bad).is_err(), "{body}");
        }
        // References within the drawing stay, and words that only contain "on" ("polygon", "points=").
        let masked = json!({ "prefix": "ph", "icons": { "x": { "body": "<mask id=\"a\"/><path mask=\"url(#a)\"/><use href=\"#a\"/>" } } }).to_string();
        assert!(save(dir.path(), "phosphor", &masked).is_ok());
        let fine = json!({ "prefix": "ph", "icons": { "x": { "body": "<polygon points=\"1 2\"/>" } } }).to_string();
        assert!(save(dir.path(), "phosphor", &fine).is_ok());
        assert!(save(dir.path(), "tabler", answer).is_err(), "another set's prefix");
        assert!(save(dir.path(), "lucide", answer).is_err(), "Lucide is bundled");

        delete(dir.path(), "phosphor").unwrap();
        assert!(downloaded(dir.path()).is_empty());
    }
}
