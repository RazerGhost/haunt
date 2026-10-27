//! Haunt's own texts for the window (errors, notices, the titles of what it did), in the window's language: Rust sends
//! a key, its values and the English, and the window words the key in its language (app/locales/<language>/rust/,
//! `said()` in app/utils/text.ts), falling back to the English for a key it does not know. What git itself says is
//! passed on as git said it.
//!
//! `said!("repos.notThere", "That folder is not there.")`, or with values
//! `said!("refs.noBranch", "There is no branch \"{name}\".", name = branch)`: the key is the file under rust/ and the
//! text's name in it; the English is the same text app/locales/en/rust/ has (shared/locales.test.ts checks).

/// Marks a said text: it starts with this, and its parts are split by it.
const MARK: char = '\u{1}';

/// A text as the window reads it: MARK key MARK values (JSON) MARK the English with the values filled in.
pub fn compose(key: &str, english: &str, values: &serde_json::Value) -> String {
    let mut filled = english.to_owned();
    if let Some(map) = values.as_object() {
        for (name, value) in map {
            filled = filled.replace(&format!("{{{name}}}"), value.as_str().unwrap_or_default());
        }
    }
    format!("{MARK}{key}{MARK}{values}{MARK}{filled}")
}

/// The English of a said text, for logs and anything that is not the window; other text as it is.
#[allow(dead_code)]
pub fn english(text: &str) -> &str {
    if text.starts_with(MARK) {
        text.rsplit(MARK).next().unwrap_or(text)
    } else {
        text
    }
}

/// A text for the window: a key, its English (with `{name}`s), and the values by name.
#[macro_export]
macro_rules! said {
    ($key:literal, $english:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        let values = serde_json::json!({ $(stringify!($name): $value.to_string()),* });
        $crate::said::compose($key, $english, &values)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_the_key_the_values_and_the_english() {
        let name = "main";
        let text = crate::said!("refs.noBranch", "There is no branch \"{name}\".", name = name);
        assert_eq!(text, "\u{1}refs.noBranch\u{1}{\"name\":\"main\"}\u{1}There is no branch \"main\".");
        assert_eq!(english(&text), "There is no branch \"main\".");
        assert_eq!(english("git: plain"), "git: plain");
        assert!(crate::said!("a.b", "Plain.").ends_with("\u{1}{}\u{1}Plain."));
    }
}
