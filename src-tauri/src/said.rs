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

/// A text as the window reads it: MARK key MARK values (JSON) MARK the English with the values filled in. A value
/// that is itself a said text goes in as its English, so its marks never reach the window.
pub fn compose(key: &str, english: &str, values: &serde_json::Value) -> String {
    let mut values = values.clone();
    if let Some(map) = values.as_object_mut() {
        for value in map.values_mut() {
            if let Some(text) = value.as_str().filter(|text| text.starts_with(MARK)) {
                *value = serde_json::Value::String(self::english(text).to_owned());
            }
        }
    }
    let mut filled = english.to_owned();
    if let Some(map) = values.as_object() {
        for (name, value) in map {
            filled = filled.replace(&format!("{{{name}}}"), value.as_str().unwrap_or_default());
        }
    }
    format!("{MARK}{key}{MARK}{values}{MARK}{filled}")
}

/// The English of a said text, for logs and anything that is not the window; other text as it is.
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

    #[test]
    fn a_said_value_goes_in_as_its_english() {
        let inner = crate::said!("refs.noBranch", "There is no branch \"{name}\".", name = "main");
        let text = crate::said!("a.failed", "It failed: {error}", error = inner);
        assert_eq!(
            text,
            "\u{1}a.failed\u{1}{\"error\":\"There is no branch \\\"main\\\".\"}\u{1}It failed: There is no branch \"main\"."
        );
        assert_eq!(text.matches(MARK).count(), 3);
    }
}
