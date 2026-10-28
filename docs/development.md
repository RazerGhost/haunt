# Working on Haunt

## Running it

```bash
pnpm install
pnpm app
```

`pnpm app` starts the dev server (`pnpm dev`, on 127.0.0.1:4777) and the app around it, with hot reload. In debug
builds the app also answers its calls on 127.0.0.1:4778, and the dev server forwards `/__haunt` there, so the window
runs in a browser at http://127.0.0.1:4777 too, with its dev tools. Only Haunt's own page on this machine gets an
answer, and a release build has none of this.

`pnpm app:build` builds an installer. It needs Rust and, on Windows, the Visual Studio C++ Build Tools ("Desktop
development with C++"); on Linux the WebKitGTK libraries listed in `.github/workflows/release.yml`. A version tag
(`git tag v0.2.0 && git push --tags`), or running the Release workflow by hand, builds every platform on GitHub Actions
and attaches the installers to a draft release, with `docs/releases/<tag>.md` as its notes when that file exists.
Publishing that release packages it for Arch as `haunt-bin` (`.github/workflows/aur.yml`): the PKGBUILD in
`packaging/aur/haunt-bin` gets the version and the `.deb`'s checksums and is built and installed on Arch; the package
is attached to the release, and the PKGBUILD is pushed to the AUR. Edit the package there, not in the AUR. The AUR
step needs the secret `AUR_SSH_KEY`, the private half of an SSH key on the AUR account that maintains `haunt-bin`;
without it that step says so and stops. For a release published before, run the AUR workflow by hand with its tag.

## How it is put together

- **`src-tauri/`**: the app, in Rust. The window calls typed Tauri commands (`api.rs`); `app/bindings.ts` is written
  from them by `cargo test`. Every call about a repository names it, and `repos.rs` refuses one for a repository that
  is not shown, so an answer cannot land in the wrong tab.
  - `git/` reads repositories: a commit, the files between two commits and a file's diff on gix, in the process
    (`objects.rs`), and the log, refs, blame and `log -L` through the `git` program with every command of the
    repository's config switched off (`cli.rs`).
  - `git/write/` is the only place that writes: a fixed list of operations, one at a time per repository, as the
    folder's trust allows, with discards saved first; a clone or `git init` (`create.rs`) is the one write in a folder
    that is no tab yet. `askpass.rs` passes git's questions to the window.
  - `git/command.rs` is the one way git is started (reads, writes, network runs), with the safety rules of each in
    one place; `process.rs` runs programs with a time limit and stops them with everything they started (a job
    object on Windows, a process group on Unix).
  - `code/read/` has one file per language behind the `Grammar` trait; adding a language is one file and one line.
  - `git/overview.rs` reads any repository by its path, open or not, for the repository manager, and `folders.rs`
    finds the repositories in the folders you add.
  - `code/` reads PHP, Rust, Lua, Python, Go, Java, C#, Ruby, C and C++ with tree-sitter (`read.rs`, calls matched
    by name in `guess.rs`), which the scanner asks for while it scans (`others.rs`), maps a change onto the functions of both versions of each
    file (`changes.rs`), and reads WordPress routes (`routes.rs`). Without a JavaScript runtime, `scan.rs` scans
    those languages itself, putting the scan together as the sidecar does.
  - `repos.rs` keeps the tabs, trust and the last scans (`settings.rs` saves them), `watch.rs` watches the shown
    repository, `sidecar.rs` runs the scanners, `editor.rs` opens your editor, `startup.rs` finds Node, Bun or Deno
    and checks git, `hosts.rs` asks `gh`, `glab` or `tea`, and `tryit.rs` sends Try it's requests to local dev
    servers.
- **`sidecar/`**: the scanner, plain TypeScript that Node runs directly (Node 22.18 or later), or else Bun or Deno
  (`HAUNT_RUNTIME=bun` or `deno` picks one), one process per shown repository, spoken to over stdin and stdout with
  one JSON message per line (`main.ts`, then `serve.ts`).
  `functions.ts` builds the tree of functions per file, `calls.ts` resolves every call with the type checker,
  `project.ts` decides what to read, `imports.ts` builds the import graph, `routes.ts` finds Hono and Express routes,
  and `scan.ts` puts them together. `read.ts` reads other versions of a file for the changes by function, answered at
  once even while a scan works out its call graph.
- **`app/`**: the window, built with Nuxt UI to static files the app loads; graphs are drawn with Cytoscape and code is
  coloured with Shiki. `utils/bridge.ts` gives it the generated commands; the composables hold the window's state and
  are started in order by `plugins/haunt.client.ts`.
- **`shared/`**: what both sides use, with their tests: the types, the commit graph's lanes, word diffs, the commit
  message drafts, release notes, themes and more.

## Checks

Run these before a change:

```bash
pnpm test
pnpm typecheck
pnpm build
pnpm check:rust
```

`check:rust` checks the Rust's formatting, runs clippy with its warnings as errors, and runs the Rust tests. The Rust
tests start the real scanner on fixture projects (`sidecar/fixtures/`), so they need Node (or Bun or Deno) too. One test asks gitlab.com
for real through `glab` and is skipped unless asked for: `cargo test asks_gitlab_through_glab -- --ignored`.

After adding an `i-lucide-*` icon, run `node scripts/icon-sets.mjs` to update the icon sets' matches.
