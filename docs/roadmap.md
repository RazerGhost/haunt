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

On Windows and macOS Haunt's own themes fit; on Linux people theme the whole desktop (GTK and Qt themes, an accent
colour, wallpaper tools such as pywal, matugen and wallust), and a fixed look stands out. Haunt follows the desktop by
default, takes a theme file for those who generate their colours, and can use the system's title bar. In this order:

1. **Any colour, not only the palette's.** Needed by 2 and 3: a desktop accent or a pywal colour is any colour, and
   Appearance takes palette names only (`PRIMARY` and `NEUTRAL` in settings.rs). Let each colour be a hex as well as a
   name; shared/theme.ts makes its 50–950 ramp in OKLCH, spaced the way Tailwind's palettes are, for light and dark;
   settings.rs `Appearance::checked` accepts a valid hex. Appearance gets a colour picker beside the named colours.
   Tests: the ramp of a known colour, contrast between the steps the window uses for text and backgrounds, and that a
   palette name gives the same as today.
2. **Follow the desktop** (Linux).
   - Light or dark, and the accent, from the freedesktop settings portal: `ReadOne` on `org.freedesktop.appearance`
     (`color-scheme`: 0 no preference, 1 dark, 2 light; `accent-color`: red, green, blue from 0 to 1, out of range
     meaning none), and `SettingChanged` to follow a change. zbus 5.19 is in the dependency tree already (the
     notification plugin): add it at that version and use its blocking API on a thread of its own. Rust sends what it
     reads as news.
   - Mode "system" uses the portal's answer, not the webview's `prefers-color-scheme`: WebKitGTK follows only GTK's
     own setting, which Hyprland and Sway often leave unset. Without a portal (a bare window manager): `gsettings get
     org.gnome.desktop.interface color-scheme`, then the webview.
   - A main-colour choice "System accent", shown when the portal has one.
   - Font: check first what `system-ui` gives in WebKitGTK (fontconfig's default sans); only if it is not the
     desktop's font, read `font-name` from the same gsettings schema.
3. **A theme file.** `theme.json` in the folder Haunt keeps its settings in (`$XDG_CONFIG_HOME/haunt` on Linux),
   with the fields Appearance has, all optional. It wins over the profile's own choices, and Appearance marks each
   field it sets ("from theme.json") so a choice that seems to do nothing is explained. The settings folder is watched
   (watch.rs, the folder alone), and a change applies at once; a file that does not read keeps the last good one and
   says why, once. docs/theming.md gives the fields and a template each for pywal, matugen and wallust. Applies on
   every system, though it is mostly for Linux.
4. **The system's title bar** (Windows and Linux): Settings, Appearance, "Use the system's title bar", for the whole
   app. The window takes the system's frame (`set_decorations(true)`, live), Haunt's minimise, maximise and close go,
   and the tabs stay as a bar under it. Off by default; the help says a tiling compositor draws no title bar either way.

Each step is tried on a real Linux machine before the next: GNOME (portal, accent), KDE (portal, server-side title
bar), and Hyprland (portal from xdg-desktop-portal-hyprland or -gtk; a theme file from matugen). Left out: reading a
GTK or Qt theme's own widget colours and shapes, which differs per theme; the portal and a theme file cover what
people use.

## 8. Signed installers and updates

Last, since these wait on the signing applied for in step 1.

- **Signed installers.** Windows through SignPath; macOS needs the Apple Developer Program.
- **Updating in place.** After signing: unsigned installers set off SmartScreen and Gatekeeper on every update.
