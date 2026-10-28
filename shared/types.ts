/* What the scanner reports and the viewer draws. Paths are relative to the project's root, with forward slashes.
 * What Rust answers itself (the tabs, refs, log, commits, diffs and the changes by function) is typed in app/bindings.ts,
 * written from its commands. */

export type {
  BaseChoice, BranchRef, ChangedFile, ChangedFunction, ChangedPath, Changes, CommitDetail, CommitInfo, Decoration, DiffHunk, DiffLine, FileDiffView, FunctionHistory,
  Hotspots, LogCommit, LogPage, LogQuery, RangeChanges, Refs, RemoteBranchRef, RootInfo, Source, StashRef, TagRef, Wanted
} from '../app/bindings.ts'

export type FunctionKind = 'function' | 'arrow' | 'method' | 'class'

export interface FunctionNode {
  /** `path:line:column`, unique in a scan. */
  id: string
  name: string
  kind: FunctionKind
  line: number
  endLine: number
  exported: boolean
  async: boolean
  /** An unnamed callback, kept because named functions sit inside it. */
  anonymous: boolean
  /** A test's callback in a test file (`it('…', () => …)`), named by the test's title. */
  test?: boolean
  children: FunctionNode[]
}

export interface ScannedFile {
  path: string
  part: string
  test: boolean
  lines: number
  functions: FunctionNode[]
}

/** The bands of the layer map, top to bottom: the project's own layers, then the bands of the parts outside them. */
export interface Layer {
  id: string
  label: string
  note: string
  /** A layer of the project's rules (haunt.config.json or its script): the import map draws it as a row. */
  ruled?: boolean
}

export interface Part {
  name: string
  layer: string
  role: string
  /** Parts of its own layer it may use (in the project's own rules). */
  uses: string[]
  /** Where its files are, for the header. */
  folder: string
}

export interface CallEdge {
  from: string
  to: string
  /** How many call sites. */
  count: number
  /** Matched by name, without a type checker (PHP, Rust, Lua): likely, not certain. */
  guessed?: boolean
}

export type Rule = 'layer' | 'door'

export interface FileImport {
  from: string
  to: string
  typeOnly: boolean
  /** The rules of decision 0004 it breaks. */
  breaks: Rule[]
  /** Listed in `scripts/architecture/baseline.json`: known, may only shrink. */
  known: boolean
}

/** All imports from one core part into another. */
/** An import of one file by another in the same part. */
export interface InnerImport {
  from: string
  to: string
  typeOnly: boolean
}

export interface PartImport {
  from: string
  to: string
  files: FileImport[]
}

export interface Scan {
  root: string
  /** The project; its name is its package name or its folder's. */
  project: { kind: 'typescript', name: string }
  /** The folders, relative to the root, the server watches for changes. */
  watch: string[]
  /** Folders whose changes restart the scanner (the project's rules in code, which Node loads once). */
  restartOn: string[]
  /** Whether the folder's own code was allowed to run (its TypeScript, its architecture script). */
  trusted: boolean
  scannedAt: string
  /** Milliseconds for the functions and imports, and for the call graph. */
  ms: { structure: number, calls: number | null }
  layers: Layer[]
  parts: Part[]
  files: ScannedFile[]
  /** Null while the call graph is still being worked out. */
  calls: CallEdge[] | null
  imports: PartImport[]
  /** The imports within each part, file to file, for the views inside a part; null when the project's script works out
   * the imports itself (then only the ones between parts are known). Test files are left out. */
  innerImports: InnerImport[] | null
  /** Runtime import cycles, as file lists. */
  cycles: string[][]
  /** Whose rules judged the imports: the project's haunt.config.json, the script it names, or none. */
  rules: 'config' | 'script' | null
  /** Set when the project's rules could not be read (a broken config, a failing script): it is drawn without them. */
  architectureError: string | null
}

export type ScanEvent =
  | { type: 'root', root: string | null }
  | { type: 'trust', trusted: boolean }
  | { type: 'scanning', reason: string }
  | { type: 'scan', scannedAt: string, complete: boolean }
  | { type: 'error', message: string }
  /** Something in .git changed (a commit, a branch switch, staging, a fetch, a stash): which parts. */
  | { type: 'git', parts: GitPart[] }
  /** A fetch, pull or push: git's latest progress line, then done. */
  | { type: 'network', op: 'fetch' | 'pull' | 'push', text?: string, done?: boolean }
  /** A clone into a new folder (the repository manager): git's latest progress line, then done. */
  | { type: 'clone', text?: string, done?: boolean }
  /** git asks something during a fetch, pull or push (askpass.rs): a secret is typed as dots. */
  | { type: 'ask', id: number, prompt: string, secret: boolean, yesNo: boolean }
  /** git stopped waiting for that answer. */
  | { type: 'asked', id: number }
  /** A background fetch brought new commits to the current branch's upstream. */
  | { type: 'fetched', commits: number }
  /** Something Haunt could not do for the window outside a repository's work: a folder it was asked to open. */
  | { type: 'notice', message: string }
  /** A write that moved history went through: what it did, and what Undo would do (git/write/last.rs). */
  | { type: 'done', title: string, undo: string }
  /** A background search found a different list of repositories in a repository folder (folders.rs). */
  | { type: 'folders' }
  /** The desktop's light or dark, accent or window buttons changed (desktop.rs, Linux). */
  | { type: 'desktop', desktop: DesktopLook }
  /** theme.json changed, or stopped reading (theme_file.rs); its fields are app/bindings.ts `ThemeFileState`. */
  | { type: 'themeFile', state: { path: string, theme: Record<string, unknown> | null, problem: string | null, leftOut: string[] } }

/** What the desktop says about its look (desktop.rs), each null when it says nothing. */
export interface DesktopLook {
  dark: boolean | null
  accent: string | null
  buttons: { left: string[], right: string[] } | null
}

/** A part of git; each view reads again only for the parts it shows. */
export type GitPart = 'head' | 'refs' | 'index' | 'stash'

export const EDITORS = ['vscode', 'cursor', 'windsurf', 'zed', 'webstorm', 'sublime', 'custom'] as const
export type Editor = (typeof EDITORS)[number]
