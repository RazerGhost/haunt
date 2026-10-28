# Roadmap

What comes after the first release, in the order we mean to build it. Steps marked done are built and say what is left
of them; check the code before starting on an item, since parts may exist by then. [features.md](features.md)
describes what Haunt does today.

## 1. Start what takes outside waiting time

These cost little work but wait on someone else, so they start first.

- **Apply for signing.** Windows through the SignPath Foundation's free signing for open-source projects (apply under
  the name Haunt). Approval takes weeks; the signed installers themselves come at the end.
- **A first full release** (not a pre-release). Haunt checks for a new version and links to it, but GitHub's "latest
  release" skips pre-releases, so the check finds nothing until one exists.
- **Linux, on a real machine:** check that the window draws (some NVIDIA and Wayland setups need
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`, which Haunt could set itself), that saving a file rescans, and that fetch and
  push work.

## 2. Translations

Done: the window's texts are in app/locales (English and Dutch), looked up by `t()` in app/utils/text.ts; Haunt's own
texts from Rust come as keys with values (`said!`, src-tauri/src/said.rs) and are worded in the window. What git itself
says stays as git says it.

## 3. Repository manager

Done: the repository manager (app/components/RepositoryManager.vue), clone and `git init`, and managing remotes, from
the sidebar and from the manager.

## 4. Reflog

Done (app/pages/reflog.vue, src-tauri/src/git/reflog.rs).

## 5. Git

Done: ignoring files and stashing some from Local changes, side-by-side diffs there too, new pull requests through
`gh` and `glab`, searching files at a commit, and signed commits.

## 6. Code map

Done: bookmarked files, who changed each function last, exporting the canvas and the layer map, and Java, C#, Ruby,
C and C++ (C# property reads, operators and event accessors; C++ conversion operators; Ruby locals by position). What
reading by name cannot see stays out: C# implicit conversions and operators whose operand types the source does not
show, indexer use (`order[1]`), and operator uses in C++.

## 7. Fit in on Linux

Built, not yet tried on Linux: any colour as a hex with its shades (shared/ramp.ts) and a colour picker in Appearance;
the desktop's light or dark, accent and window buttons from the settings portal or gsettings (src-tauri/src/desktop.rs);
theme.json with roles, mode, font and opacity, watched (src-tauri/src/theme_file.rs, docs/theming.md); and "Use the
system's title bar". In a Debian container the Linux code passes clippy and its own tests; what the container cannot
show (a portal, a compositor, WebKitGTK drawing) is left, on a real machine:

- **Hyprland** (CachyOS with Quickshell, xdg-desktop-portal-hyprland and -gtk): dark from gsettings through the
  portal, `button-layout` `appmenu:maximize,close`, theme.json from matugen on each wallpaper change, and `opacity`
  (a transparent WebKitGTK window on the AMD iGPU and under `prime-run`).
- **GNOME** (the accent, followed as it changes) and **KDE** (the accent, the system's title bar).
- **The font**: what `system-ui` gives in WebKitGTK (fontconfig's default sans); if it is not the desktop's, read
  `font-name` from `org.gnome.desktop.interface`. theme.json's `font` covers it meanwhile.

Left out: reading a GTK or Qt theme's own widget colours and shapes, which differs per theme; the portal and a theme
file cover what people use.

## 8. Signed installers and updates

Last, since these wait on the signing applied for in step 1.

- **Signed installers.** Windows through SignPath; macOS needs the Apple Developer Program.
- **Updating in place.** After signing: unsigned installers set off SmartScreen and Gatekeeper on every update.
