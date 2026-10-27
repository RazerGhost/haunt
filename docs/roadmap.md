# Roadmap

What comes after the first release, in the order we mean to build it. Nothing here is built yet; check the code before
starting on an item, since parts may exist by then. [features.md](features.md) describes what Haunt does today.

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

The window's texts are in app/locales (English and Dutch), looked up by `t()` in app/utils/text.ts. What is left:

- **Texts that come from Rust** are still English: errors, the operation banner's title, the undo notices, and the
  trust labels and notes (repos.rs), which show up inside Dutch sentences. Send them as codes with values, as
  shared/risk.ts now does, and word them in the window.

## 3. Repository manager

Done: the repository manager (app/components/RepositoryManager.vue), clone and `git init`, and managing remotes from
the sidebar. What is left:

- **Remotes in the manager itself**: the manager shows them, but adding, renaming and removing happen in the sidebar
  of an open tab, where writes have the tab's trust.

## 4. Reflog

- A view of where each branch and HEAD pointed, to get back any lost commit or branch tip. Undo goes back one step
  only.

## 5. Git

Smallest first.

- **Ignore a file** from Local changes (add it to `.gitignore`).
- **Stash only some files** rather than everything.
- **Side-by-side diffs**; diffs are one column today.
- **Create pull requests** (`gh pr create`, `glab mr create`); today Haunt only reads them and their status.
- **Search file contents at a commit** (`git grep`).
- **Signed commits:** show which commits carry a signature and whether it checks out.

## 6. Code map

- **Bookmark files** (pinning functions exists already).
- **Export** the canvas or the layer map as PNG or SVG.
- **Blame per function:** the last author and age of each function in the function panel.
- **More languages** through tree-sitter: Java, C#, Ruby, C and C++.

## 7. Signed installers and updates

Last, since these wait on the signing applied for in step 1.

- **Signed installers.** Windows through SignPath; macOS needs the Apple Developer Program.
- **Updating in place.** After signing: unsigned installers set off SmartScreen and Gatekeeper on every update.
