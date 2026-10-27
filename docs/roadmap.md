# Roadmap

What comes after the first release, in rough order. Nothing here is built yet; check the code before starting on an
item, since parts may exist by then. [features.md](features.md) describes what Haunt does today.

## Next

- **Translations.** Every text in the window is English, written straight into the components. Move the texts into
  language files behind a lookup, add Dutch, and a language setting. Choose between `@nuxtjs/i18n` (a new dependency)
  and a small lookup helper of our own with JSON files.
- **A repository manager** in place of today's open screen, like Fork's. On the left, the recent repositories and all
  repositories grouped by folder, as a tree. On the right, the one picked: uncommitted files, commit count, first and
  last commit, its remotes with links (the host, issues, pull requests), its branches, tags and latest commits with
  author and age, a statistics tab, and its README.
- **Clone and create.** Clone from a URL, and make a new repository (`git init`); Haunt only opens folders that exist.
  These belong on the repository manager.
- **Reflog.** A view of where each branch and HEAD pointed, to get back any lost commit or branch tip. Undo goes back
  one step only.

## Git

- **Create pull requests** (`gh pr create`, `glab mr create`); today Haunt only reads them and their status.
- **Manage remotes:** add, rename, remove, and change a remote's URL.
- **Ignore a file** from Local changes (add it to `.gitignore`).
- **Stash only some files** rather than everything.
- **Search file contents at a commit** (`git grep`).
- **Side-by-side diffs**; diffs are one column today.
- **Signed commits:** show which commits carry a signature and whether it checks out.

## Code map

- **Export** the canvas or the layer map as PNG or SVG.
- **Blame per function:** the last author and age of each function in the function panel.
- **Bookmark files** (pinning functions exists already).
- **More languages** through tree-sitter: Java, C#, Ruby, C and C++.

## Releases

- **Signed installers.** Windows through the SignPath Foundation's free signing for open-source projects (apply under
  the name Haunt); macOS needs the Apple Developer Program.
- **Updating in place.** Today Haunt checks for a new version and links to it. A first full release (not a
  pre-release) is needed before the check finds anything, since GitHub's "latest release" skips pre-releases.
- **Linux, on a real machine:** check that the window draws (some NVIDIA and Wayland setups need
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`, which Haunt could set itself), that saving a file rescans, and that fetch and
  push work.
