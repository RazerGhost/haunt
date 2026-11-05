// No console window behind the app in a release build on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started by git or ssh to ask something (askpass.rs): pass the question on to the app and print its answer.
    if let Some(code) = haunt_lib::askpass_main() {
        std::process::exit(code);
    }
    // WebKitGTK draws a blank window on some NVIDIA drivers under Wayland unless its DMA-BUF renderer is off. Set here,
    // before the app starts any thread: the environment is not safe to change while another thread may read it.
    #[cfg(target_os = "linux")]
    if needs_dmabuf_off(|name| std::env::var_os(name), std::path::Path::new("/proc/driver/nvidia").exists()) {
        std::env::set_var(DMABUF_RENDERER, "1");
    }
    haunt_lib::run()
}

/// WebKitGTK's switch for its DMA-BUF renderer.
#[cfg(any(target_os = "linux", test))]
const DMABUF_RENDERER: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

/// Whether to switch WebKitGTK's DMA-BUF renderer off: on Wayland (`WAYLAND_DISPLAY`, or `XDG_SESSION_TYPE` wayland)
/// with an NVIDIA driver loaded, and never when the variable is set already, whatever its value: that is the user's
/// choice.
#[cfg(any(target_os = "linux", test))]
fn needs_dmabuf_off(var: impl Fn(&str) -> Option<std::ffi::OsString>, nvidia: bool) -> bool {
    let wayland = var("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
        || var("XDG_SESSION_TYPE").is_some_and(|kind| kind == "wayland");
    var(DMABUF_RENDERER).is_none() && wayland && nvidia
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(value))
    }

    #[test]
    fn switches_dmabuf_off_on_wayland_with_nvidia_only() {
        assert!(needs_dmabuf_off(env(&[("WAYLAND_DISPLAY", "wayland-1")]), true));
        assert!(needs_dmabuf_off(env(&[("XDG_SESSION_TYPE", "wayland")]), true));
        // Not NVIDIA, or not Wayland.
        assert!(!needs_dmabuf_off(env(&[("WAYLAND_DISPLAY", "wayland-1")]), false));
        assert!(!needs_dmabuf_off(env(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]), true));
        assert!(!needs_dmabuf_off(env(&[("WAYLAND_DISPLAY", "")]), true));
        assert!(!needs_dmabuf_off(env(&[]), true));
    }

    #[test]
    fn leaves_a_value_the_user_set() {
        for value in ["0", "1", ""] {
            let vars = [("WAYLAND_DISPLAY", "wayland-1"), (DMABUF_RENDERER, value)];
            assert!(!needs_dmabuf_off(env(&vars), true));
        }
    }
}
