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
C and C++. What is left, small:

- Reading a C# property (`order.Amount`) or using an operator (`a + b`) is not a call to it; C# event accessors and
  C++ conversion-operator declarations are not read; a Ruby local assigned after a call of the same name hides it.

## 7. Signed installers and updates

Last, since these wait on the signing applied for in step 1.

- **Signed installers.** Windows through SignPath; macOS needs the Apple Developer Program.
- **Updating in place.** After signing: unsigned installers set off SmartScreen and Gatekeeper on every update.
