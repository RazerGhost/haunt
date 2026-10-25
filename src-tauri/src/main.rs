// No console window behind the app in a release build on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started by git or ssh to ask something (askpass.rs): pass the question on to the app and print its answer.
    if let Some(code) = haunt_lib::askpass_main() {
        std::process::exit(code);
    }
    haunt_lib::run()
}
