//! Opening a file at a line in the editor of your choice, through the editor's own command line tool, inside its
//! project: the project's folder goes along, so the editor shows the whole project (and reuses a window that has it
//! open). The file must be one the scan found (repos.rs checks), so the window cannot make this open anything else.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Args = fn(&str, &str, u32, u32) -> Vec<String>;

/// The editors Haunt knows, by the name the window uses, with their command and arguments (root, file, line, column).
const LAUNCHES: [(&str, &str, Args); 6] = [
    ("vscode", "code", |root, file, line, column| vec![root.into(), "-g".into(), format!("{file}:{line}:{column}")]),
    ("cursor", "cursor", |root, file, line, column| vec![root.into(), "-g".into(), format!("{file}:{line}:{column}")]),
    ("windsurf", "windsurf", |root, file, line, column| {
        vec![root.into(), "-g".into(), format!("{file}:{line}:{column}")]
    }),
    ("zed", "zed", |root, file, line, column| vec![root.into(), format!("{file}:{line}:{column}")]),
    ("webstorm", "webstorm", |root, file, line, column| {
        vec![root.into(), "--line".into(), line.to_string(), "--column".into(), column.to_string(), file.into()]
    }),
    ("sublime", "subl", |root, file, line, column| vec![root.into(), format!("{file}:{line}:{column}")]),
];

/// Characters cmd.exe would read as its own, in arguments it has to run through on Windows.
const UNSAFE: [char; 9] = ['"', '%', '^', '&', '|', '<', '>', '!', '\n'];

/// Opens a folder in the system's file manager (Explorer, Finder, or what xdg-open picks).
pub fn reveal(folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Err(format!("{} is not there yet.", folder.display()));
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(program)
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| format!("The file manager could not start: {error}"))
}

/// Opens a terminal in a folder: HAUNT_TERMINAL when set (a command, started in the folder), else Windows Terminal or
/// PowerShell on Windows, Terminal on macOS, and the first of the usual terminals found on Linux. The folder goes as
/// the working directory, not inside a command line.
pub fn terminal(folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Err(format!("{} is not there.", folder.display()));
    }
    let start = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .current_dir(folder)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    };
    if let Some(custom) = std::env::var("HAUNT_TERMINAL").ok().filter(|value| !value.trim().is_empty()) {
        let mut parts = custom.split_whitespace();
        let program = parts.next().unwrap_or_default();
        let args: Vec<&str> = parts.collect();
        return if start(program, &args) {
            Ok(())
        } else {
            Err(format!("HAUNT_TERMINAL (\"{custom}\") could not start."))
        };
    }
    let started = if cfg!(windows) {
        // A new console of its own: `start` opens one, in the folder it is started in.
        (locate("wt").is_some() && start("wt", &["-d", "."]))
            || start("cmd", &["/C", "start", "", "powershell", "-NoLogo"])
    } else if cfg!(target_os = "macos") {
        start("open", &["-a", "Terminal", "."])
    } else {
        ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "alacritty", "kitty", "xterm"]
            .iter()
            .any(|program| locate(program).is_some() && start(program, &[]))
    };
    if started {
        Ok(())
    } else {
        Err("No terminal could be started. Set HAUNT_TERMINAL to the one you use, e.g. \"wezterm\".".into())
    }
}

/// Opens a web page in the system's browser; only `https://` addresses.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        return Err("Only web addresses (https) can be opened.".into());
    }
    let mut command = if cfg!(windows) {
        // Through the shell's URL handler: `start` would read & and ^ in the address as its own.
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    } else {
        let mut command = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        command.arg(url);
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| format!("The browser could not start: {error}"))
}

/// A command's path on the PATH, as `where` (Windows) or `which` finds it.
fn locate(command: &str) -> Option<PathBuf> {
    let mut finder = Command::new(if cfg!(windows) { "where" } else { "which" });
    finder.arg(command).stdin(Stdio::null());
    crate::startup::hide_console(&mut finder);
    let output = finder.output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).lines().map(str::trim).find(|line| !line.is_empty()).map(PathBuf::from)
}

/// The custom editor's command (HAUNT_EDITOR), e.g. `nvim-qt +{line} {file}` ({root} is the project folder).
fn custom() -> Option<String> {
    std::env::var("HAUNT_EDITOR").ok().filter(|value| !value.trim().is_empty())
}

/// The editors whose command line tool is on the PATH, and "custom" when HAUNT_EDITOR is set.
pub fn installed() -> Vec<String> {
    let mut found: Vec<String> = LAUNCHES
        .iter()
        .filter(|(_, command, _)| locate(command).is_some())
        .map(|(name, _, _)| name.to_string())
        .collect();
    if custom().is_some() {
        found.push("custom".into());
    }
    found
}

/// The command and its arguments for an editor.
fn launch(editor: &str, root: &str, file: &str, line: u32, column: u32) -> Result<(String, Vec<String>), String> {
    if editor == "custom" {
        let template = custom()
            .ok_or("Set HAUNT_EDITOR to a command, e.g. \"nvim-qt +{line} {file}\" ({root} is the project folder).")?;
        let mut parts = template.split_whitespace();
        let command = parts.next().unwrap_or_default().to_string();
        let args = parts
            .map(|arg| {
                arg.replace("{root}", root)
                    .replace("{file}", file)
                    .replace("{line}", &line.to_string())
                    .replace("{column}", &column.to_string())
            })
            .collect();
        return Ok((command, args));
    }
    let (_, command, args) = LAUNCHES
        .iter()
        .find(|(name, _, _)| *name == editor)
        .ok_or_else(|| format!("Haunt does not know the editor \"{editor}\"."))?;
    Ok((command.to_string(), args(root, file, line, column)))
}

pub fn open(editor: &str, root: &str, file: &Path, line: u32, column: u32) -> Result<(), String> {
    let file = file.to_string_lossy();
    let (command, args) = launch(editor, root, &file, line, column)?;
    let found = locate(&command).ok_or_else(|| {
        format!("\"{command}\" is not on your PATH. Install the editor's command line tool, or pick another editor.")
    })?;
    // Windows runs the editors' .cmd launchers through cmd.exe, so every argument is quoted and checked first.
    let through_shell = cfg!(windows)
        && found.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));
    let mut process = if through_shell {
        let all: Vec<String> = std::iter::once(found.to_string_lossy().into_owned()).chain(args).collect();
        if all.iter().any(|arg| arg.contains(UNSAFE)) {
            return Err("That path has characters the Windows launcher cannot pass on safely.".into());
        }
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(all.iter().map(|arg| format!("\"{arg}\"")).collect::<Vec<_>>().join(" "));
        shell
    } else {
        let mut direct = Command::new(&found);
        direct.args(&args);
        direct
    };
    process.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::startup::hide_console(&mut process);
    process.spawn().map(|_| ()).map_err(|error| format!("Could not start {command}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_each_editors_arguments() {
        let (command, args) = launch("vscode", "C:/repo", "C:/repo/a.ts", 3, 1).unwrap();
        assert_eq!(command, "code");
        assert_eq!(args, ["C:/repo", "-g", "C:/repo/a.ts:3:1"]);
        let (_, args) = launch("webstorm", "/r", "/r/a.ts", 7, 2).unwrap();
        assert_eq!(args, ["/r", "--line", "7", "--column", "2", "/r/a.ts"]);
        assert!(launch("notepad", "/r", "/r/a.ts", 1, 1).is_err());
    }
}
