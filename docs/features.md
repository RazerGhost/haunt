# Haunt's features

Everything Haunt does, by part of the window. The [README](../README.md) has the short version.

- [The window](#the-window)
- [Git](#git)
- [Changes by function](#changes-by-function)
- [History](#history)
- [The code views](#the-code-views)
- [Tools](#tools)
- [How Haunt reads a project](#how-haunt-reads-a-project)
- [Your own layer rules](#your-own-layer-rules)
- [Trust and safety](#trust-and-safety)
- [What it leaves out](#what-it-leaves-out)

## The window

- **Tabs**, one per repository, with a + to open another, a recent one, or the tab closed last (Ctrl Shift T). Drag a
  tab to move it; right-click it to pin it (first in the row, only its icon, no close button) or give it a colour, to
  close it, the others or those to its right, to show its folder, to open a terminal there (`HAUNT_TERMINAL` picks
  which), or to move it to another profile. The tabs, the screen each one was on and the window's size are kept between launches. A tab in the
  background does not scan; going back to it shows its last scan at once while it scans again.
- **The repository manager**, shown when no repository is open and from the + beside the tabs. On the left, the
  recent repositories with each one's branch, uncommitted files and last commit, and the repositories in the folders
  you add (looked through four levels deep) as a tree of their folders; typing filters both (↑ ↓ to pick, Enter or a
  double click to open), and a typed path says whether it is a project or a git repository before it opens. On the
  right, the one picked, open in a tab or not: its uncommitted files, commit count, first and last commit, its remotes
  with their project, issues and pull request pages, its branches, tags and latest commits; Statistics (commits per
  month over the last year, who committed most, the files by extension); and its README, drawn by Haunt itself
  (no HTML of the file's own, no images, and only the remotes' pages open from its links). **Clone** a URL into a
  new folder (git's progress as it comes, a way to stop it, its questions asked as for a fetch; no hooks or
  submodules run) or make a **New** empty repository (`git init`); either opens when done. `haunt <folder>` opens a
  folder, in the window that is already running if there is one, and a folder dropped on the window opens in its tab.
- **Git and Code**, the two halves of the window (Ctrl 1 and Ctrl 2). The toolbar has Fetch, Pull and Push (with the
  commits to pull and push, and git's progress with a way to stop it), Branch and Stash, the function search, and the
  scan, trust and settings icons.
- **The sidebar**: in Git, Local changes, All commits, Pull requests and a More menu (Reflog, Bisect, Release notes, Apply
  patches, Health), then the branches as folders split on `/` with what each is ahead and behind, worktrees,
  submodules, remotes, tags and stashes. A filter narrows long lists, and the eraser beside Branches cleans up merged
  branches and those whose remote branch is gone. The + beside Remotes adds a remote; right-click one to copy or
  change its URL, rename it or remove it (with its remote branches; nothing changes on the remote itself). In Code, the code views and the project's parts by layer.
- **The command palette** (Ctrl K): one search over the actions, the screens, the branches, the open repositories and
  the functions by name. A sha typed there goes to its commit.
- **The function panel**: a function's code, where it is written, what it calls and what calls it, and its history.
  Pin a function to keep it a click away. In a diff, the function a hunk is in opens in the panel.
- **Open in your editor**: every function and file opens at its line, with the project's folder, so the editor shows
  the whole project. VS Code, Cursor, Windsurf, Zed, WebStorm and Sublime Text work through their command line tools;
  for anything else set `HAUNT_EDITOR`, such as `nvim-qt +{line} {file}` (`{root}` is the project's folder).
- **When something fails**, the message says what git said, with Copy, and offers to trust the folder when its trust
  was the reason. After a discard, a dropped stash, an undone commit or a reset, Undo shows for a few seconds; the
  Recently discarded and Recently dropped lists keep them for 30 days either way. Further back, the **Reflog** (More,
  or the palette) lists where HEAD or a branch pointed, newest first, with what moved it, and marks the commits only
  the reflog still keeps (left behind by a reset, a rebase or an amend, or by a deleted branch); right-click one to see
  it, put a new branch on it, reset the current branch to it or cherry-pick it.
- **Notifications**: a fetch, pull, push or test run that finishes while Haunt is in the background says so through
  the system.
- **Keyboard**: Ctrl K the palette, Ctrl F the function search, F5 fetch, Ctrl Shift P pull, Ctrl Shift U push, Alt ←
  and → back and forward, Ctrl Enter commit; in Local changes ↑ ↓ move through the files and Space stages or unstages.
  ⌘ takes the place of Ctrl on macOS.
- **Profiles**: groups of repositories (work, your own, a client's), each with its own tabs, recent list and look,
  switched from the title bar. Trusted folders and the other settings are shared.
- **Appearance**, per profile: Haunt's theme or one of Nuxt UI's twelve, then your own choices on top (light, dark or
  the system's; the main colour and the greys, apart for light and for dark; the other colours; corners, font, text
  size, icons, buttons, fields, panels and control size). Fonts are not bundled: a theme's font shows when it is
  installed, and can be loaded from coolLabs Fonts if you switch that on. Lucide icons come with Haunt; another set
  (Bootstrap, Phosphor, Tabler, Remix, Material, Heroicons, Iconoir) is downloaded once when you pick it, checked, and
  kept on your computer.
- **Settings**: the language (English or Dutch; it starts as the system's), test files shown or not, the editor,
  diffs (wrapping, tab width, whitespace), commit times and order, what a pull does when the branches went apart,
  fetching in the background, sign-off by default, pull request status, the update check, and the trusted folders.

## Git

Haunt reads git, and writes only what you ask for.

- **The commit list** (the home screen): every branch, remote branch and tag with the graph, newest first, by topology
  or by date. Filter by branch, message, author or file, by text added or removed (`git log -S`), or merges only or
  none. Pages load as you scroll. Click a commit to see it below; Ctrl-click a second one to compare the two. A Local
  changes row above the history leads to the work not committed yet.
- **Local changes**: unstaged files above, staged files below, the diff beside them and the commit box under them.
  Stage, unstage or discard whole files or single lines; the functions the changes are in show above the diff, and a
  click picks all of a function's lines. A discard is saved first and can be brought back for 30 days. The commit box
  has amend and sign-off, keeps a draft per branch, counts the subject's length, offers recent subjects, and says when
  hooks or signing will not run because the folder is not trusted, or when amending rewrites a commit that is on a
  remote already. **Draft** writes a message from what is staged, by function, such as "Entries: adds resolveSlug;
  changes createEntry; 2 callers in Api", worked out on your machine without AI. Activity lists every git command Haunt
  ran to write. The diff is unified or side by side (one choice for every diff), and lines are picked in either.
  Right-click a file to stash just it, or, for an untracked one, to ignore it, every file with its extension or its
  folder (added to the repository's `.gitignore`); **Stash some files** stashes the files you tick.
- **Branches, stashes and tags**: create a branch anywhere and switch to it (taking your changes along or stashing
  them), check out a remote branch, rename and delete (saying when commits would be lost), stash with or without
  untracked files (or only the files you pick, from Local changes), apply, pop and drop, and create, delete and push
  tags.
- **Fetch, pull and push**: fetch every remote; pull with fast-forward, merge or rebase (Haunt asks when the branches
  went apart); push to the tracked branch or a new one, and overwrite only if the remote branch is still where your
  last fetch saw it. **Before pulling**, the incoming commits can be read by function, with the functions you changed
  too marked: where a pull may conflict, seen before it happens.
- **Undoing and merging**: cherry-pick, revert, reset (soft, mixed or hard; a hard reset saves your changes first),
  undo the last commit, merge a branch (or drag it onto yours), rebase (drag yours onto another), and rebase
  interactively: reorder commits and keep, reword, squash, fix up or drop each, with no editor. Haunt says when a commit
  you are about to change is on a remote already.
- **Search files** (More, the palette, or a commit's menu): the files at any commit that hold some text, a regular
  expression or a whole word (`git grep`), by file, with each line around a match shown as it was then.
- **Signed commits**: the commit list marks the commits that carry a signature, and a commit's details check it (good,
  key not trusted, expired, revoked, bad, or not checkable here) with your own gpg or ssh-keygen settings: a
  repository's own config cannot pick the program that checks, or the signers it trusts.
- **Undo** for the last write that moved history: a commit or amend, merge, pull, revert, cherry-pick, reset, rebase,
  branch switch, or a branch made, renamed or deleted. A toast says what Undo would do, and the palette offers it while
  nothing has moved since.
- **Conflicts**: a merge, revert, cherry-pick, pull or rebase that stops on conflicts shows a banner with Continue and
  Abort. The conflict view shows your lines and theirs side by side per block, to take yours, theirs or both, or edit
  the result.
- **Pull requests**, reviewed without switching your branch: fetched from the remote as refs (GitHub, GitLab and Gitea
  offer them), with no account, and read with every lens of the commit list. With pull request status switched on,
  each branch shows its pull request and checks through `gh`, `glab` or `tea` (Gitea is untested so far). **New pull
  request** makes one (a merge request on GitLab) through the same tool, signed in as you: the branch it goes into, a
  title and description made from its commits, and draft or not; a branch not on the remote yet is pushed first.
- **Worktrees**: more branches checked out side by side, each in its own folder and tab.
- **Submodules**: listed with whether each is checked out at the recorded commit, opened in their own tab, and updated
  (in a trusted folder, since updating clones from the addresses in `.gitmodules`).
- **Open on the web, or copy the link**: a commit, a branch, a file, or a function's lines, on GitHub, GitLab,
  Bitbucket or Gitea.

## Changes by function

Every commit, two commits compared, a pull request, a branch and the local changes can be read by function.

- **Files**: the changed files and their diffs, unified or side by side, with syntax colours, the words that changed
  marked, and the function each hunk is in.
- **Functions**: what was added, changed or removed, read from both versions. A function moved to another file, given
  another name, or put into a class shows once, as moved, with where it came from.
- **What could break**: the functions that call a changed one, one to three steps out, and **the tests that reach the
  change**: the tests whose calls lead to it, and the changed functions no test reaches. In a trusted folder, Run runs
  just those tests with the project's own runner (Vitest, Jest or node's).
- **Risk**: one line that says low, medium or high, and why: how many functions call what changed, how many changed
  functions no test reaches, the busy files it touches, and its size.
- **Who should review**: who wrote the lines the change touches, leaving out you and the change's authors.
- **Architecture**: the imports a change adds and removes, marked when they break the project's rules or close a
  cycle.
- **Map and calls**: the change on the layer map or the canvas, and the changed functions with their callers.
- **Review a branch**: the functions your branch changed since it left main, one at a time, each with its code, what
  calls it and the diff; Mark seen ticks them off.

## History

- **A function's history**: who wrote its lines, when they changed, and the commits that touched it (`git log -L`),
  followed across renames and moves to other files. It shows the commit it was born in and the last one that changed
  how it works, past the commits that only changed spacing or comments.
- **A file's history and blame**: every commit that touched a file, across renames, and blame for the file now or at
  any commit, with a way to step back past a line's last change.
- **Who knows it**: per part or folder, who wrote what it has now and who changed it in the last year, with a warning
  when one person knows it alone.
- **Hotspots**: parts and files tinted by how often they changed in the last 180 days.
- **Architecture over time**: the rule breaks each of the last 50 commits added and removed.

## The code views

- **Layers**: the project's layers, top to bottom, with a tile per part; a part opens folder by folder down to its
  functions. **Last changed** shows who changed each function last and when (a `git blame` of each file shown open).
  Bookmark a file to keep it a click away in the Code sidebar and the palette, next to the pinned functions.
- **Canvas**: the code as boxes in boxes, each sized by its lines: parts, folders, files and functions. Zoom in for
  names and then code; hover a function to see its calls as lines.
- **Export** the whole canvas or layer map, as shown (light or dark, test files or not), as a PNG or SVG image.
- **Calls**: how the code reaches a function, from its entry points down, or its callers and callees around it.
- **Imports**: the imports between parts, on the project's layers when it has rules, with breaks and runtime cycles;
  a part opens to the imports between its folders.
- **Unused**: functions nothing uses, exports nothing in the scan uses, and exports used only in their own file, each
  with "When?" for about when its last use went.
- **Routes**: the HTTP routes found in the code (Hono and Express, with their prefixes across files, and WordPress's
  `register_rest_route`), each with its middleware, its handler and what the handler calls. Call path opens the handler
  on the Calls page. **Try it** sends a request to your dev server on this machine and shows the answer beside the
  functions the route goes through; requests can be saved in the repository as `.http` files.

Haunt watches the project's folders and `.git`, and rescans when something changes. The call graph is kept between
launches, so a repository that did not change has it at once.

## Tools

In the sidebar's More menu and the palette:

- **Bisect**: find the commit that broke something, step by step, with the commits that touched a chosen function
  suggested first.
- **Release notes** between two tags: the commits grouped by the part of the code they changed, with what each part's
  functions gained and lost, as Markdown to edit.
- **Patches**: save commits as patch files, and apply patches after seeing what they change by function.
- **Health**: what the history weighs, the largest files across every version, files that belong in Git
  LFS, and old branches.
- **Updates**: Check for updates in Settings, and an optional daily check.

## How Haunt reads a project

Without configuration:

- **Languages**: TypeScript and JavaScript, and the script blocks of Vue, Svelte and Astro files, read with the
  TypeScript compiler, with the project's tsconfig, path aliases and workspace packages. PHP (Laravel, WordPress), Rust,
  Lua, Python, Go, Java, C#, Ruby, C and C++ (a `.h` header as C), read with tree-sitter: their imports are resolved
  per language (Composer's PSR-4 map, Rust's modules and crates, Lua's `require`, Python's imports, Go's packages
  through go.mod, Java's imports from the package's source folder, C#'s `using` to the namespace's folder, Ruby's
  `require` and `require_relative`, C and C++'s `#include "…"`) and their calls are matched by name, drawn dashed and
  marked ≈. WordPress hooks count as calls to their callback.
- **Files**: what the tsconfigs include, and every other source file outside dependencies, builds and bundled
  JavaScript.
- **Parts**: in a workspace (pnpm, npm or Cargo), each package or crate; otherwise the folders in the source folder
  (`src/`, `app/`, `lib/`) or at the root.
- **Tests** are found by their names and folders in every language, and are left out of the imports.

## Your own layer rules

A `haunt.config.json` at the project's root draws the project on its own layers and checks its imports. Reading it
runs no code.

```json
{
  "layers": [
    { "id": "app", "label": "Application" },
    { "id": "domain", "note": "the business rules" },
    { "id": "infra" }
  ],
  "parts": [
    { "match": "src/app", "layer": "app" },
    { "match": "src/domain/*", "name": "domain/$1", "layer": "domain", "uses": ["domain/shared"] },
    { "match": "src/infra/*", "layer": "infra", "role": "storage" }
  ],
  "entries": ["index.ts"]
}
```

- `layers`, top to bottom. A part may import the parts of lower layers, and the parts of its own layer it lists in
  `uses`; anything else breaks the layer rule.
- `parts`: folder globs (`*` is one folder, `**` any number). The first rule that matches makes that folder a part;
  `name` may use the globs' matches (`$1`, `$2`…).
- `entries`: the files of a part that other parts may import; importing any other file of it breaks the entry rule.

A project whose rules already live in code can name a script instead, run only in a trusted folder:
`"architecture": { "script": "tools/haunt.ts" }`. Its default export gets `{ root, files }` and returns:

```ts
{
  layers: [{ id: 'ui', label: 'Interface' }, { id: 'core' }],
  parts: [{ name: 'core', layer: 'core', role: 'the logic', uses: [] }],
  partOf: (path) => path.startsWith('src/core/') ? 'core' : null,
  judge: (from, to) => from.part === 'core' && to.part === 'ui' ? ['layer'] : [],
  // or, instead of judge, its own judged imports and cycles, with `known` for breaks it accepts for now:
  // imports: [{ from, to, typeOnly, breaks: ['door'], known: true }], cycles: [['a.ts', 'b.ts']]
}
```

Haunt rescans when the file or the script changes.

## Trust and safety

Haunt reads a project without running anything from it. Trusting a folder lets it also run the project's own code:

- its TypeScript (`node_modules/typescript`), instead of the copy Haunt ships;
- the architecture script its `haunt.config.json` names;
- its git hooks, wherever git finds them (`core.hooksPath`, as Husky sets it, or the git folder).

When a folder has code like that, a banner asks once. See what it runs shows each hook and script first.

When reading, git runs with everything a repository's own config could make it run switched off: diff drivers,
textconv, filters, fsmonitor and pagers. Haunt takes no lock on the index, so it never gets in your way.

When writing, a trusted folder runs git as a terminal would: hooks, filters such as Git LFS, and signing. In an
untrusted folder, hooks and signing are off, and staging, switching or stashing files that need a filter is refused.
Pull and push run only in a trusted folder. Fetch runs anywhere; in an untrusted folder, what the repository's own
config could make it run is switched off, while your own git settings still apply. Haunt writes one thing at a time
per repository, waits for git's `index.lock` but never deletes it, and lists every command it ran under Activity.

Haunt never stores a password. git uses your own credential manager, keychain and SSH agent, and its questions show in
Haunt's window, with the answer going straight back to git.

## What it leaves out

- Calls made through an interface stop at the interface, and calls into packages that were not scanned are not drawn.
- The templates of Vue, Svelte and Astro files: a function called only from markup has no caller.
- In the languages read with tree-sitter, a call whose name several functions share, through a variable whose type
  only a checker would know, is left out rather than guessed.
- Route prefixes passed into a function that registers routes are not followed.
