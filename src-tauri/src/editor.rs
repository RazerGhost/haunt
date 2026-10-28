//! Opening a file at a line in the editor of your choice, through the editor's own command line tool, inside its
//! project: the project's folder goes along, so the editor shows the whole project (and reuses a window that has it
//! open). The file must be one the scan found (repos.rs checks), so the window cannot make this open anything else.
//! Also the file manager, a terminal in a folder, and web pages in the browser.
//!
//! Programs are found on the PATH by Haunt itself, not through `which` (a separate package on Arch), and on macOS and
//! Linux also on the PATH the login shell sets up and in the folders installers use without adding them to it (an app
//! started from the desktop gets neither): ~/.local/bin, JetBrains Toolbox's scripts, Flatpak's exported commands.
//! Everything started runs on its own and is waited for aside, so it never stays behind as a zombie.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Args = fn(&str, &str, u32, u32) -> Vec<String>;

/// The editors Haunt knows, by the name the window uses: the commands each may go by, the first found wins (Arch's
/// Zed is `zeditor`, a Flatpak's command is its app id), and its arguments (root, file, line, column).
const EDITORS: [(&str, &[&str], Args); 8] = [
    ("vscode", &["code", "code-oss", "com.visualstudio.code"], vscode_like),
    ("vscodium", &["codium", "vscodium", "com.vscodium.codium"], vscode_like),
    ("cursor", &["cursor"], vscode_like),
    ("windsurf", &["windsurf"], vscode_like),
    ("zed", &["zed", "zeditor", "dev.zed.Zed"], |root, file, line, column| {
        vec![root.into(), format!("{file}:{line}:{column}")]
    }),
    ("webstorm", &["webstorm", "com.jetbrains.WebStorm"], |root, file, line, column| {
        vec![root.into(), "--line".into(), line.to_string(), "--column".into(), column.to_string(), file.into()]
    }),
    ("sublime", &["subl", "com.sublimetext.three"], |root, file, line, column| {
        vec![root.into(), format!("{file}:{line}:{column}")]
    }),
    // Kate finds the project from the file's repository itself.
    ("kate", &["kate", "org.kde.kate"], |_, file, line, _| vec!["--line".into(), line.to_string(), file.into()]),
];

fn vscode_like(root: &str, file: &str, line: u32, column: u32) -> Vec<String> {
    vec![root.into(), "-g".into(), format!("{file}:{line}:{column}")]
}

/// Editors that run inside a terminal, by program name: given as $VISUAL or $EDITOR, Haunt opens a terminal for them.
const TERMINAL_EDITORS: [&str; 9] = ["vi", "vim", "nvim", "nano", "micro", "hx", "helix", "kak", "emacs"];

/// The terminals Haunt knows: the arguments that start one in `{folder}`, and the ones before a command it should run
/// there. Each also starts in the folder as its working directory.
const TERMINALS: [(&str, &[&str], &[&str]); 12] = [
    ("xdg-terminal-exec", &[], &[]),
    ("x-terminal-emulator", &[], &["-e"]),
    ("kitty", &["--directory", "{folder}"], &["--directory", "{folder}"]),
    ("foot", &["--working-directory", "{folder}"], &["--working-directory", "{folder}"]),
    ("ghostty", &["--working-directory={folder}"], &["--working-directory={folder}", "-e"]),
    ("wezterm", &["start", "--cwd", "{folder}"], &["start", "--cwd", "{folder}", "--"]),
    ("alacritty", &["--working-directory", "{folder}"], &["--working-directory", "{folder}", "-e"]),
    ("konsole", &["--workdir", "{folder}"], &["--workdir", "{folder}", "-e"]),
    ("gnome-terminal", &["--working-directory={folder}"], &["--working-directory={folder}", "--"]),
    ("xfce4-terminal", &["--working-directory={folder}"], &["--working-directory={folder}", "-x"]),
    ("xterm", &[], &["-e"]),
    ("wt", &["-d", "{folder}"], &["-d", "{folder}"]),
];

/// Characters cmd.exe would read as its own, in arguments it has to run through on Windows.
const UNSAFE: [char; 9] = ['"', '%', '^', '&', '|', '<', '>', '!', '\n'];

/// The editor and terminal Settings, General names; empty is none set.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    /// The custom editor's command line, e.g. `nvim +{line} {file}`.
    pub editor: String,
    /// The custom editor runs inside a terminal (Neovim, Helix).
    pub editor_in_terminal: bool,
    /// The terminal's command line, e.g. `kitty` or `wezterm start --cwd {folder}`.
    pub terminal: String,
}

/// Opens a folder in the system's file manager (Explorer, Finder, or what xdg-open picks).
pub fn reveal(folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Err(said!("editor.notThereYet", "{folder} is not there yet.", folder = folder.display()));
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = crate::process::command(program);
    command.arg(folder);
    detached(command)
        .map_err(|error| said!("editor.fileManager", "The file manager could not start: {error}", error = error))
}

/// Opens a terminal in a folder: the one Settings names, else HAUNT_TERMINAL, else $TERMINAL, else Windows Terminal or
/// PowerShell on Windows, Terminal on macOS, and the first of the usual terminals found on Linux.
pub fn terminal(tools: &Tools, folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Err(said!("editor.notThere", "{folder} is not there.", folder = folder.display()));
    }
    in_terminal(tools, folder, None)
}

/// Starts a terminal in `folder`, running `run` (a found program and its arguments) when given.
fn in_terminal(tools: &Tools, folder: &Path, run: Option<Vec<String>>) -> Result<(), String> {
    let folder_text = folder.to_string_lossy();
    let start = |words: Vec<String>| {
        let (program, args) = words.split_first()?;
        let found = locate(program)?;
        let mut command = crate::process::command(found);
        command.args(args).current_dir(folder);
        detached(command).ok()
    };
    if let Some(chosen) = chosen_terminal(tools) {
        let line = terminal_words(&words(&chosen), &folder_text, run.as_deref());
        return start(line).ok_or_else(|| {
            said!("editor.customTerminal", "The terminal \"{custom}\" could not start.", custom = chosen)
        });
    }
    let known = |name: &str| terminal_words(&[name.to_owned()], &folder_text, run.as_deref());
    let started = if cfg!(windows) {
        start(known("wt")).is_some()
            || match &run {
                // A new console of its own: `start` opens one, in the folder it is started in.
                None => start(words("cmd /C start \"\" powershell -NoLogo")).is_some(),
                Some(run) => {
                    let quoted = cmd_quoted(run)?;
                    start(vec!["cmd".into(), "/C".into(), format!("start \"\" {quoted}")]).is_some()
                }
            }
    } else if cfg!(target_os = "macos") {
        if run.is_some() {
            return Err(said!(
                "editor.macTerminal",
                "Terminal cannot be given an editor to run: set a terminal in Settings, General (kitty, WezTerm, Alacritty or Ghostty)."
            ));
        }
        start(words("open -a Terminal .")).is_some()
    } else {
        TERMINALS.iter().filter(|(name, _, _)| *name != "wt").any(|(name, _, _)| start(known(name)).is_some())
    };
    if started {
        Ok(())
    } else {
        Err(said!(
            "editor.noTerminal",
            "No terminal could be started. Name the one you use in Settings, General, e.g. \"wezterm\"."
        ))
    }
}

/// The terminal you chose: Settings, else HAUNT_TERMINAL, else $TERMINAL (Linux and macOS).
fn chosen_terminal(tools: &Tools) -> Option<String> {
    let set = |value: String| Some(value).filter(|value| !value.trim().is_empty());
    set(tools.terminal.clone()).or_else(|| std::env::var("HAUNT_TERMINAL").ok().and_then(set)).or_else(|| {
        if cfg!(windows) {
            None
        } else {
            std::env::var("TERMINAL").ok().and_then(set)
        }
    })
}

/// A terminal's command line, as words, with the folder and the command to run filled in. A terminal Haunt knows,
/// named alone, gets its own arguments; otherwise `{folder}` and `{command}` are filled in where they are, and a
/// command with no `{command}` to go in goes last, after `-e` (or the known terminal's own).
fn terminal_words(chosen: &[String], folder: &str, run: Option<&[String]>) -> Vec<String> {
    let Some(program) = chosen.first() else { return Vec::new() };
    let known = TERMINALS.iter().find(|(name, _, _)| name_of(program) == *name);
    let mut out = vec![program.clone()];
    let mut placed = false;
    for word in &chosen[1..] {
        if word == "{command}" {
            out.extend(run.unwrap_or_default().iter().cloned());
            placed = true;
        } else {
            out.push(word.replace("{folder}", folder));
        }
    }
    let alone = chosen.len() == 1;
    match (run, known) {
        (Some(run), _) if !placed => {
            let before: Vec<String> = match known {
                Some((_, _, before)) if alone => before.iter().map(|word| word.replace("{folder}", folder)).collect(),
                _ => vec!["-e".into()],
            };
            out.extend(before);
            out.extend(run.iter().cloned());
        }
        (None, Some((_, opens, _))) if alone => out.extend(opens.iter().map(|word| word.replace("{folder}", folder))),
        _ => {}
    }
    out
}

/// Opens a web page in the system's browser; only `https://` addresses.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        return Err(said!("editor.httpsOnly", "Only web addresses (https) can be opened."));
    }
    let command = if cfg!(windows) {
        // Through the shell's URL handler: `start` would read & and ^ in the address as its own.
        let mut command = crate::process::command("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    } else {
        let mut command = crate::process::command(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        command.arg(url);
        command
    };
    detached(command).map_err(|error| said!("editor.browser", "The browser could not start: {error}", error = error))
}

/// Starts a program that runs on its own, and waits for it aside, so it does not stay behind as a zombie (Unix).
fn detached(mut command: Command) -> std::io::Result<()> {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// A command's path: as given when it is one, else the first on the PATH (with the login shell's and the installers'
/// folders, `search_folders`); on Windows as `where` finds it, which knows the launchers' .cmd and .exe.
fn locate(command: &str) -> Option<PathBuf> {
    if command.contains('/') || (cfg!(windows) && command.contains('\\')) {
        return Some(PathBuf::from(command)).filter(|path| path.is_file());
    }
    if cfg!(windows) {
        let mut finder = crate::process::command("where");
        finder.arg(command).stdin(Stdio::null());
        crate::startup::hide_console(&mut finder);
        let output = finder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        return String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(PathBuf::from);
    }
    search_folders().iter().map(|folder| folder.join(command)).find(|path| runnable(path))
}

#[cfg(unix)]
fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(path: &Path) -> bool {
    path.is_file()
}

/// Where programs are looked for, once: Haunt's PATH, the login shell's, and the folders installers put commands in.
fn search_folders() -> &'static [PathBuf] {
    static FOLDERS: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    FOLDERS.get_or_init(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let at_home = |path: &str| home.as_ref().map(|home| home.join(path));
        let own = std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect::<Vec<_>>());
        let shell = login_shell_path().map(|path| std::env::split_paths(&path).collect::<Vec<_>>());
        let extra = [
            at_home(".local/bin"),
            at_home(".local/share/JetBrains/Toolbox/scripts"),
            at_home("Library/Application Support/JetBrains/Toolbox/scripts"),
            at_home(".local/share/flatpak/exports/bin"),
            Some(PathBuf::from("/var/lib/flatpak/exports/bin")),
            Some(PathBuf::from("/opt/homebrew/bin")),
            Some(PathBuf::from("/usr/local/bin")),
        ];
        let mut folders: Vec<PathBuf> = Vec::new();
        for folder in own.into_iter().flatten().chain(shell.into_iter().flatten()).chain(extra.into_iter().flatten()) {
            if folder.is_absolute() && !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
    })
}

/// The PATH your login shell sets up (fish, zsh and bash alike), which an app started from the desktop does not get.
fn login_shell_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty())?;
    let mut command = crate::process::command(shell);
    // Marked, since a shell's start-up files may print their own lines first.
    command.args(["-ilc", "printf '\\nhaunt-path=%s\\n' \"$PATH\""]).stdin(Stdio::null());
    let output = crate::process::run(command, std::time::Duration::from_secs(5)).ok()?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.lines().rev().find_map(|line| line.strip_prefix("haunt-path=")).map(str::to_owned)
}

/// A program's name without its folder or extension: `/usr/bin/nvim` → `nvim`.
fn name_of(program: &str) -> &str {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    name.strip_suffix(".exe").unwrap_or(name)
}

/// A command line's words: spaces part them, and quotes ('' or "") keep spaces inside one, with no other escapes.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                started = false;
            }
            (None, c) => word.push(c),
        }
    }
    if started || !word.is_empty() {
        out.push(word);
    }
    out
}

/// The custom editor: the one Settings names, else HAUNT_EDITOR, else $VISUAL or $EDITOR; and whether it runs inside
/// a terminal (as Settings says, or for $VISUAL and $EDITOR when it is one of the terminal editors).
fn custom(tools: &Tools) -> Option<(String, bool)> {
    if !tools.editor.trim().is_empty() {
        return Some((tools.editor.clone(), tools.editor_in_terminal));
    }
    let set = |name: &str| std::env::var(name).ok().filter(|value| !value.trim().is_empty());
    if let Some(command) = set("HAUNT_EDITOR") {
        return Some((command, false));
    }
    let command = set("VISUAL").or_else(|| set("EDITOR"))?;
    let terminal = words(&command).first().is_some_and(|program| TERMINAL_EDITORS.contains(&name_of(program)));
    Some((command, terminal))
}

/// The editors found, and "custom" when one is set.
pub fn installed(tools: &Tools) -> Vec<String> {
    let mut found: Vec<String> = EDITORS
        .iter()
        .filter(|(_, commands, _)| commands.iter().any(|command| locate(command).is_some()))
        .map(|(name, _, _)| name.to_string())
        .collect();
    if custom(tools).is_some() {
        found.push("custom".into());
    }
    found
}

/// The command and its arguments for an editor, and whether it runs inside a terminal.
fn launch(
    tools: &Tools,
    editor: &str,
    root: &str,
    file: &str,
    line: u32,
    column: u32,
) -> Result<(String, Vec<String>, bool), String> {
    if editor == "custom" {
        let (template, terminal) = custom(tools).ok_or_else(|| {
            said!(
                "editor.noCustom",
                "Set the custom editor's command in Settings, General, e.g. \"nvim-qt +{line} {file}\" ({root} is the project folder)."
            )
        })?;
        // Never empty: `custom` gives only a command with something in it.
        let mut parts = words(&template);
        let command = parts.remove(0);
        // A bare editor ($EDITOR=nvim) gets the file and line the way it takes them.
        if !parts.iter().any(|part| part.contains("{file}")) {
            match name_of(&command) {
                "vi" | "vim" | "nvim" | "nano" | "micro" | "kak" | "emacs" | "emacsclient" | "gvim" | "neovide" => {
                    parts.extend(["+{line}".into(), "{file}".into()])
                }
                "hx" | "helix" => parts.push("{file}:{line}:{column}".into()),
                _ => parts.push("{file}".into()),
            }
        }
        let args = parts
            .iter()
            .map(|arg| {
                arg.replace("{root}", root)
                    .replace("{file}", file)
                    .replace("{line}", &line.to_string())
                    .replace("{column}", &column.to_string())
            })
            .collect();
        return Ok((command, args, terminal));
    }
    let (_, commands, args) = EDITORS
        .iter()
        .find(|(name, _, _)| *name == editor)
        .ok_or_else(|| said!("editor.unknown", "Haunt does not know the editor \"{editor}\".", editor = editor))?;
    let command = commands.iter().find(|command| locate(command).is_some()).unwrap_or(&commands[0]);
    Ok((command.to_string(), args(root, file, line, column), false))
}

/// Quotes a command for cmd.exe, refusing what it would read as its own.
fn cmd_quoted(all: &[String]) -> Result<String, String> {
    if all.iter().any(|arg| arg.contains(UNSAFE)) {
        return Err(said!("editor.unsafePath", "That path has characters the Windows launcher cannot pass on safely."));
    }
    Ok(all.iter().map(|arg| format!("\"{arg}\"")).collect::<Vec<_>>().join(" "))
}

pub fn open(tools: &Tools, editor: &str, root: &str, file: &Path, line: u32, column: u32) -> Result<(), String> {
    let file = file.to_string_lossy();
    let (command, args, terminal) = launch(tools, editor, root, &file, line, column)?;
    let found = locate(&command).ok_or_else(|| {
        said!(
            "editor.notOnPath",
            "\"{command}\" is not on your PATH. Install the editor's command line tool, or pick another editor.",
            command = command
        )
    })?;
    if terminal {
        let run: Vec<String> = std::iter::once(found.to_string_lossy().into_owned()).chain(args).collect();
        return in_terminal(tools, Path::new(root), Some(run));
    }
    // Windows runs the editors' .cmd launchers through cmd.exe, so every argument is quoted and checked first.
    let through_shell = cfg!(windows)
        && found.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));
    let mut process = if through_shell {
        let all: Vec<String> = std::iter::once(found.to_string_lossy().into_owned()).chain(args).collect();
        let mut shell = crate::process::command("cmd");
        shell.arg("/C").arg(cmd_quoted(&all)?);
        shell
    } else {
        let mut direct = crate::process::command(&found);
        direct.args(&args);
        direct
    };
    crate::startup::hide_console(&mut process);
    detached(process).map_err(|error| {
        said!("editor.notStarted", "Could not start {command}: {error}", command = command, error = error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn builds_each_editors_arguments() {
        let tools = Tools::default();
        let (_, args, terminal) = launch(&tools, "vscode", "C:/repo", "C:/repo/a.ts", 3, 1).unwrap();
        assert_eq!(args, ["C:/repo", "-g", "C:/repo/a.ts:3:1"]);
        assert!(!terminal);
        let (_, args, _) = launch(&tools, "webstorm", "/r", "/r/a.ts", 7, 2).unwrap();
        assert_eq!(args, ["/r", "--line", "7", "--column", "2", "/r/a.ts"]);
        let (_, args, _) = launch(&tools, "kate", "/r", "/r/a.ts", 7, 2).unwrap();
        assert_eq!(args, ["--line", "7", "/r/a.ts"]);
        assert!(launch(&tools, "notepad", "/r", "/r/a.ts", 1, 1).is_err());
    }

    #[test]
    fn fills_in_the_custom_editor_and_completes_a_bare_one() {
        let tools =
            |editor: &str, terminal| Tools { editor: editor.into(), editor_in_terminal: terminal, ..Tools::default() };
        let (command, args, terminal) =
            launch(&tools("'/opt/My Editor/ed' --goto {file}:{line}", false), "custom", "/r", "/r/a b.ts", 4, 1)
                .unwrap();
        assert_eq!(command, "/opt/My Editor/ed");
        assert_eq!(args, ["--goto", "/r/a b.ts:4"]);
        assert!(!terminal);
        let (_, args, terminal) = launch(&tools("nvim", true), "custom", "/r", "/r/a.ts", 9, 3).unwrap();
        assert_eq!(args, ["+9", "/r/a.ts"]);
        assert!(terminal);
        let (_, args, _) = launch(&tools("hx", true), "custom", "/r", "/r/a.ts", 9, 3).unwrap();
        assert_eq!(args, ["/r/a.ts:9:3"]);
        let (_, args, _) = launch(&tools("gedit", false), "custom", "/r", "/r/a.ts", 9, 3).unwrap();
        assert_eq!(args, ["/r/a.ts"]);
    }

    #[test]
    fn splits_a_command_line_as_a_shell_would() {
        assert_eq!(words("  nvim  +{line}   {file} "), ["nvim", "+{line}", "{file}"]);
        assert_eq!(words(r#""C:\Program Files\x.exe" -a '' b"#), [r"C:\Program Files\x.exe", "-a", "", "b"]);
        assert!(words("   ").is_empty());
    }

    #[test]
    fn starts_each_terminal_in_the_folder_and_runs_the_editor_in_it() {
        let run = strings(&["/usr/bin/nvim", "+3", "/r/a.ts"]);
        // A known terminal, named alone, gets its own arguments.
        assert_eq!(terminal_words(&strings(&["kitty"]), "/r", None), ["kitty", "--directory", "/r"]);
        assert_eq!(
            terminal_words(&strings(&["kitty"]), "/r", Some(&run)),
            ["kitty", "--directory", "/r", "/usr/bin/nvim", "+3", "/r/a.ts"]
        );
        assert_eq!(
            terminal_words(&strings(&["wezterm"]), "/r", Some(&run)),
            ["wezterm", "start", "--cwd", "/r", "--", "/usr/bin/nvim", "+3", "/r/a.ts"]
        );
        assert_eq!(terminal_words(&strings(&["xdg-terminal-exec"]), "/r", None), ["xdg-terminal-exec"]);
        // One with its own arguments: the folder filled in, the command where it says, or last after -e.
        assert_eq!(
            terminal_words(&strings(&["footclient", "-D", "{folder}"]), "/r", Some(&run)),
            ["footclient", "-D", "/r", "-e", "/usr/bin/nvim", "+3", "/r/a.ts"]
        );
        assert_eq!(
            terminal_words(&strings(&["/usr/bin/kitty", "--single-instance", "{command}"]), "/r", Some(&run)),
            ["/usr/bin/kitty", "--single-instance", "/usr/bin/nvim", "+3", "/r/a.ts"]
        );
        assert_eq!(terminal_words(&strings(&["myterm", "--x"]), "/r", None), ["myterm", "--x"]);
    }

    #[test]
    fn names_a_program_without_its_folder() {
        assert_eq!(name_of("/usr/bin/nvim"), "nvim");
        assert_eq!(name_of(r"C:\bin\hx.exe"), "hx");
        assert_eq!(name_of("kitty"), "kitty");
    }

    #[cfg(unix)]
    #[test]
    fn finds_a_program_without_which() {
        assert!(locate("sh").is_some_and(|path| path.is_absolute()));
        assert!(locate("haunt-no-such-program").is_none());
        assert_eq!(locate("/bin/sh"), Some(PathBuf::from("/bin/sh")));
    }
}
