//! The development bridge's arguments: `dispatch` (written by `commands!`) takes each by name from the JSON.

use serde_json::Value;

/// An argument from the development bridge's JSON, by its name as the window sends it (camelCase, as Tauri expects).
pub(super) fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    let mut camel = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            camel.extend(c.to_uppercase());
            upper = false;
        } else {
            camel.push(c);
        }
    }
    serde_json::from_value(args.get(&camel).cloned().unwrap_or(Value::Null))
        .map_err(|error| format!("\"{camel}\": {error}"))
}
