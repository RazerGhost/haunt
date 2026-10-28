//! The desktop's own look, which Haunt follows on Linux: light or dark, the accent colour, and the window buttons
//! (which ones, on which side). Read from the freedesktop settings portal (`org.freedesktop.portal.Settings`, over
//! D-Bus), whose `SettingChanged` says when one changes; each change goes to the window as the news `desktop`. Without
//! a portal (a bare window manager), `gsettings` once at start. Elsewhere nothing is read: Windows and macOS give the
//! webview their light or dark themselves.
//!
//! - Light or dark: `org.freedesktop.appearance` `color-scheme`, 0 no preference, 1 dark, 2 light. The webview's own
//!   `prefers-color-scheme` follows only GTK's setting, which tiling compositors often leave unset.
//! - Accent: `org.freedesktop.appearance` `accent-color`, red, green and blue from 0 to 1; out of range is none.
//! - Window buttons: `org.gnome.desktop.wm.preferences` `button-layout`, as GTK apps read it: `appmenu:maximize,close`
//!   is maximise and close on the right.

use parking_lot::Mutex;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;

use crate::events::Events;

/// What the desktop says; each field null when it says nothing.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Type)]
pub struct Desktop {
    /// Dark (true) or light (false).
    pub dark: Option<bool>,
    /// The accent colour, `#rrggbb`.
    pub accent: Option<String>,
    /// The window buttons in order on each side: "minimize", "maximize", "close".
    pub buttons: Option<Buttons>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Type)]
pub struct Buttons {
    pub left: Vec<String>,
    pub right: Vec<String>,
}

static DESKTOP: Mutex<Option<Desktop>> = Mutex::new(None);

/// What is known now (nothing until the first read, and nothing outside Linux).
pub fn current() -> Desktop {
    DESKTOP.lock().clone().unwrap_or_default()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn update(events: &Events, change: impl FnOnce(&mut Desktop)) {
    let next = {
        let mut known = DESKTOP.lock();
        let mut next = known.clone().unwrap_or_default();
        change(&mut next);
        if known.as_ref() == Some(&next) {
            return;
        }
        *known = Some(next.clone());
        next
    };
    events.send(None, serde_json::json!({ "type": "desktop", "desktop": next }));
}

/// Reads the desktop's look and follows it, on a thread of its own (Linux; elsewhere nothing).
pub fn start(events: Arc<Events>) {
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || portal::follow(&events));
    #[cfg(not(target_os = "linux"))]
    let _ = events;
}

/// `color-scheme`'s number as dark or light.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scheme(value: u32) -> Option<bool> {
    match value {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// `accent-color`'s red, green and blue as a hex; none when out of range (the portal's "no accent").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn accent(red: f64, green: f64, blue: f64) -> Option<String> {
    let channels = [red, green, blue];
    if channels.iter().any(|channel| !(0.0..=1.0).contains(channel)) {
        return None;
    }
    Some(format!(
        "#{}",
        channels.iter().map(|channel| format!("{:02x}", (channel * 255.0).round() as u8)).collect::<String>()
    ))
}

/// `button-layout` ("appmenu:maximize,close"): the buttons Haunt draws, on each side; none when it names none.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn buttons(layout: &str) -> Option<Buttons> {
    let side = |text: &str| -> Vec<String> {
        text.split(',')
            .map(str::trim)
            .filter(|name| ["minimize", "maximize", "close"].contains(name))
            .map(str::to_owned)
            .collect()
    };
    let (left, right) = layout.split_once(':').unwrap_or(("", layout));
    let found = Buttons { left: side(left), right: side(right) };
    (!found.left.is_empty() || !found.right.is_empty() || layout.contains(':')).then_some(found)
}

/// gsettings' answer without its quotes: `'prefer-dark'` is `prefer-dark`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unquoted(text: &str) -> &str {
    text.trim().trim_matches('\'')
}

/// gsettings' `color-scheme` as dark or light.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn gsettings_scheme(text: &str) -> Option<bool> {
    match unquoted(text) {
        "prefer-dark" => Some(true),
        "prefer-light" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
mod portal {
    use super::{accent, buttons, gsettings_scheme, scheme, unquoted, update};
    use crate::events::Events;
    use std::time::Duration;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    const APPEARANCE: &str = "org.freedesktop.appearance";
    const WM: &str = "org.gnome.desktop.wm.preferences";

    /// What a setting says, for the field it belongs to.
    fn apply(events: &Events, namespace: &str, key: &str, value: &Value) {
        // Older portals answer `Read` with the value inside one more variant.
        let value = match value {
            Value::Value(inner) => inner.as_ref(),
            other => other,
        };
        match (namespace, key, value) {
            (APPEARANCE, "color-scheme", Value::U32(number)) => {
                update(events, |desktop| desktop.dark = scheme(*number))
            }
            (APPEARANCE, "accent-color", Value::Structure(fields)) => {
                let channels: Vec<f64> = fields
                    .fields()
                    .iter()
                    .filter_map(|field| match field {
                        Value::F64(channel) => Some(*channel),
                        _ => None,
                    })
                    .collect();
                if let [red, green, blue] = channels[..] {
                    update(events, |desktop| desktop.accent = accent(red, green, blue));
                }
            }
            (WM, "button-layout", Value::Str(layout)) => {
                update(events, |desktop| desktop.buttons = buttons(layout.as_str()))
            }
            _ => {}
        }
    }

    /// One setting from the portal; false when it has none (no portal, or not that key).
    fn read(events: &Events, proxy: &Proxy, namespace: &str, key: &str) -> bool {
        let answer: zbus::Result<OwnedValue> =
            proxy.call("ReadOne", &(namespace, key)).or_else(|_| proxy.call("Read", &(namespace, key)));
        match answer {
            Ok(value) => {
                apply(events, namespace, key, &value);
                true
            }
            Err(_) => false,
        }
    }

    /// A key from gsettings, when the portal has none.
    fn gsettings(schema: &str, key: &str) -> Option<String> {
        let mut command = crate::process::command("gsettings");
        command.args(["get", schema, key]);
        let finished = crate::process::run(command, Duration::from_secs(3)).ok()?;
        finished.status.success().then(|| String::from_utf8_lossy(&finished.stdout).into_owned())
    }

    fn from_gsettings(events: &Events, scheme_too: bool, layout_too: bool) {
        if scheme_too {
            if let Some(text) = gsettings("org.gnome.desktop.interface", "color-scheme") {
                update(events, |desktop| desktop.dark = gsettings_scheme(&text));
            }
        }
        if layout_too {
            if let Some(text) = gsettings(WM, "button-layout") {
                update(events, |desktop| desktop.buttons = buttons(unquoted(&text)));
            }
        }
    }

    pub fn follow(events: &Events) {
        let proxy = Connection::session().and_then(|bus| {
            Proxy::new(
                &bus,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
            )
        });
        let Ok(proxy) = proxy else {
            from_gsettings(events, true, true);
            return;
        };
        // Listening first, so a change while reading is not missed.
        let changes = proxy.receive_signal("SettingChanged");
        let has_scheme = read(events, &proxy, APPEARANCE, "color-scheme");
        read(events, &proxy, APPEARANCE, "accent-color");
        let has_layout = read(events, &proxy, WM, "button-layout");
        from_gsettings(events, !has_scheme, !has_layout);
        let Ok(changes) = changes else { return };
        for message in changes {
            if let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, OwnedValue)>() {
                apply(events, &namespace, &key, &value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_portals_answers() {
        assert_eq!(scheme(1), Some(true));
        assert_eq!(scheme(2), Some(false));
        assert_eq!(scheme(0), None);
        assert_eq!(accent(0.2078, 0.5176, 0.8941).as_deref(), Some("#3584e4"));
        assert_eq!(accent(-1.0, -1.0, -1.0), None, "out of range is no accent");
        assert_eq!(gsettings_scheme("'prefer-dark'\n"), Some(true));
        assert_eq!(gsettings_scheme("'default'\n"), None);
    }

    #[test]
    fn reads_the_window_buttons_as_gtk_does() {
        let found = buttons("appmenu:maximize,close").unwrap();
        assert_eq!(found.left, Vec::<String>::new());
        assert_eq!(found.right, ["maximize", "close"]);
        let mac_like = buttons("close,minimize,maximize:").unwrap();
        assert_eq!(mac_like.left, ["close", "minimize", "maximize"]);
        assert!(mac_like.right.is_empty());
        assert_eq!(buttons("icon:").map(|found| found.left.len()), Some(0), "no buttons at all, said on purpose");
        assert_eq!(buttons(""), None);
        assert_eq!(buttons(unquoted("'appmenu:minimize,maximize,close'\n")).unwrap().right.len(), 3);
    }
}
