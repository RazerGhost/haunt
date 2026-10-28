# Theming Haunt from your desktop

Haunt's look is set per profile in Settings, Appearance. On Linux it also follows the desktop, and a theme file lets a
wallpaper tool colour it along with the rest of your desktop.

## Following the desktop (Linux)

- **Light or dark.** With Appearance's "System", Haunt asks the desktop's settings portal
  (`org.freedesktop.appearance` `color-scheme`) and follows it as it changes. Without a portal it reads
  `gsettings get org.gnome.desktop.interface color-scheme` once at start. On Hyprland, Sway and other compositors
  without a settings app, set it with:

  ```bash
  gsettings set org.gnome.desktop.interface color-scheme prefer-dark
  ```

- **The accent colour.** When the portal has one (GNOME 47 and later, KDE), Appearance offers "System accent" as a main
  colour. The GTK portal only knows GNOME's named accents; a wallpaper tool's colours come through the theme file
  instead.
- **The window buttons.** Haunt draws the buttons `org.gnome.desktop.wm.preferences` `button-layout` names, on its
  side, as GTK apps do: `appmenu:maximize,close` is maximise and close on the right, `close,minimize:` is close and
  minimise on the left. Without it, a tiling compositor gets only Close, and other desktops all three on the right.
- **The system's title bar.** Settings, Appearance, "Use the system's title bar" puts the desktop's own frame above
  the tabs (Windows and Linux). A tiling compositor draws no title bars, so there it changes nothing.

## theme.json

Haunt reads `theme.json` from a `haunt` folder in your config folder:

| System  | Path                                          |
| ------- | --------------------------------------------- |
| Linux   | `~/.config/haunt/theme.json`                  |
| Windows | `%APPDATA%\haunt\theme.json`                  |
| macOS   | `~/Library/Application Support/haunt/theme.json` |

What it sets wins over every profile's own choices, and Appearance marks those fields "from theme.json". Haunt
follows the file as it changes: a tool that writes it recolours Haunt at once, with no command to run after. A file
that does not read keeps the last version that did, and Appearance says why. A field Haunt does not know the value of
is left out, and Appearance names it.

Every field is optional:

```json
{
  "mode": "dark",
  "primary": "#d0bcff",
  "neutral": "#141218",
  "error": "#f2b8b5",
  "font": "Inter",
  "opacity": 0.85,
  "roles": {
    "background": "#141218",
    "raised": "#211f26",
    "text": "#e6e0e9",
    "muted": "#cac4d0",
    "border": "#49454f",
    "primary": "#d0bcff",
    "onPrimary": "#381e72"
  }
}
```

- **`mode`**: `light`, `dark` or `system`.
- **Colours** (`primary`, `neutral`, `secondary`, `success`, `info`, `warning`, `error`): a hex, or one of Tailwind's
  names (`blue`, `zinc`). Haunt makes the shades from it. `primary` and `neutral` are for both light and dark;
  `darkPrimary` and `darkNeutral` set dark apart.
- **`roles`**: the exact colours of the window's parts, each a hex, over the shades: `background` (the page),
  `raised` (cards, menus, the title bar), `text`, `muted` (less important text), `border`, `primary` (buttons, links,
  what is chosen) and `onPrimary` (text on it). A role left out comes from the shades.
- **`font`**: any installed font by name.
- **`opacity`**: the background from 0 (clear) to 1, so the compositor's blur shows through (Linux). The window is made
  see-through at start, so an opacity added later applies after Haunt is restarted.
- **The rest of Appearance**: `theme`, `radius` (rem: `0`, `0.125`, `0.25`, `0.375`, `0.5`, `0.625`, `0.75`, `1`),
  `fontSize` (px: `14` to `18`), `icons`, `buttons`, `inputs`, `panels`, `size`.

### matugen

`~/.config/matugen/templates/haunt.json`:

```json
{
  "mode": "{{mode}}",
  "primary": "{{colors.primary.default.hex}}",
  "neutral": "{{colors.surface.default.hex}}",
  "error": "{{colors.error.default.hex}}",
  "roles": {
    "background": "{{colors.surface.default.hex}}",
    "raised": "{{colors.surface_container.default.hex}}",
    "text": "{{colors.on_surface.default.hex}}",
    "muted": "{{colors.on_surface_variant.default.hex}}",
    "border": "{{colors.outline_variant.default.hex}}",
    "primary": "{{colors.primary.default.hex}}",
    "onPrimary": "{{colors.on_primary.default.hex}}"
  }
}
```

And in `~/.config/matugen/config.toml` (no `post_hook`: Haunt sees the file change):

```toml
[templates.haunt]
input_path  = 'templates/haunt.json'
output_path = '~/.config/haunt/theme.json'
```

### pywal

`~/.config/wal/templates/haunt.json` (pywal's templates double a brace that is not a colour):

```json
{{
  "mode": "dark",
  "primary": "{color4}",
  "neutral": "{background}",
  "roles": {{
    "background": "{background}",
    "raised": "{color0}",
    "text": "{foreground}",
    "muted": "{color7}",
    "border": "{color8}",
    "primary": "{color4}",
    "onPrimary": "{background}"
  }}
}}
```

pywal writes it to `~/.cache/wal/haunt.json`; link it once, then restart Haunt so it watches the linked file too:

```bash
mkdir -p ~/.config/haunt && ln -sf ~/.cache/wal/haunt.json ~/.config/haunt/theme.json
```

### wallust

`~/.config/wallust/templates/haunt.json`:

```json
{
  "mode": "dark",
  "primary": "{{color4}}",
  "neutral": "{{background}}",
  "roles": {
    "background": "{{background}}",
    "raised": "{{color0}}",
    "text": "{{foreground}}",
    "muted": "{{color7}}",
    "border": "{{color8}}",
    "primary": "{{color4}}",
    "onPrimary": "{{background}}"
  }
}
```

And in `~/.config/wallust/wallust.toml`:

```toml
[templates]
haunt = { template = "haunt.json", target = "~/.config/haunt/theme.json" }
```
