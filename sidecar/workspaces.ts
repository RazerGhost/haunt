import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

/*
 * The packages of a JavaScript workspace: pnpm's `pnpm-workspace.yaml`, or `workspaces` in package.json (npm, Yarn,
 * Bun). Patterns are folders with `*` (one level) and `**` (any depth); `!` patterns leave folders out.
 */

export interface WorkspacePackage {
  /** The package's folder, relative to the repository, with forward slashes. */
  folder: string
  /** Its package.json name, or its folder when it has none. */
  name: string
}

/** Folders never searched: dependencies, builds and caches. */
export const IGNORED_FOLDERS = new Set(['node_modules', '.git', 'dist', 'build', 'out', 'coverage', '.nuxt', '.output', '.next', '.svelte-kit', '.astro', '.turbo', '.cache', '.data', 'target', 'vendor', '__pycache__', '.venv', 'venv', '.tox', '.mypy_cache', '.pytest_cache', 'obj'])

/** The `packages:` list of a pnpm-workspace.yaml: enough YAML for a list of strings. */
export function pnpmPatterns(yaml: string): string[] {
  const patterns: string[] = []
  let inPackages = false
  for (const line of yaml.split(/\r?\n/)) {
    if (/^packages\s*:/.test(line)) {
      inPackages = true
      const inline = /\[(.*)\]/.exec(line)
      if (inline) return inline[1]!.split(',').map(item => item.trim().replace(/^['"]|['"]$/g, '')).filter(Boolean)
      continue
    }
    if (!inPackages) continue
    const item = /^\s+-\s*(.+?)\s*(#.*)?$/.exec(line)
    if (item) patterns.push(item[1]!.replace(/^['"]|['"]$/g, ''))
    else if (/^\S/.test(line)) inPackages = false
  }
  return patterns
}

function readJson(path: string): Record<string, unknown> | null {
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>
  } catch {
    return null
  }
}

function workspacePatterns(root: string): string[] {
  const pnpm = join(root, 'pnpm-workspace.yaml')
  if (existsSync(pnpm)) return pnpmPatterns(readFileSync(pnpm, 'utf8'))
  const workspaces = readJson(join(root, 'package.json'))?.workspaces
  if (Array.isArray(workspaces)) return workspaces.filter((item): item is string => typeof item === 'string')
  if (workspaces && typeof workspaces === 'object' && Array.isArray((workspaces as { packages?: unknown }).packages)) {
    return ((workspaces as { packages: unknown[] }).packages).filter((item): item is string => typeof item === 'string')
  }
  return []
}

/**
 * A folder glob as a regular expression: `*` is one segment's worth of characters, `**` any number of segments. With
 * `capture`, each `*` and `**` is a group, for names like `$1`.
 */
export function globToRegExp(pattern: string, capture = false): RegExp {
  const cleaned = pattern.replace(/^\.\//, '').replace(/\/+$/, '')
  const [open, close] = capture ? ['(', ')'] : ['(?:', ')']
  const source = cleaned
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/(^|\/)\*\*(\/|$)/g, (_match, before: string, after: string) => `${before}\u0000${after}`)
    // `?` before `*`: the groups `*` becomes start with `(?:`.
    .replaceAll('?', '[^/]')
    .replaceAll('*', `${open}[^/]*${close}`)
    .replace(/\u0000\//g, `(?:${open}.*${close}/)?`)
    .replace(/\/\u0000/g, `(?:/${open}.*${close})?`)
    .replace(/\u0000/g, `${open}.*${close}`)
  return new RegExp(`^${source}$`)
}

/** Folders up to `depth` levels below the root that hold a `manifest` (package.json, Cargo.toml), relative paths. */
function packageFolders(root: string, manifest = 'package.json', depth = 5): string[] {
  const found: string[] = []
  const walk = (relative: string, level: number) => {
    if (level > depth) return
    const dir = relative ? join(root, relative) : root
    let names: string[]
    try {
      names = readdirSync(dir)
    } catch {
      return
    }
    for (const name of names) {
      if (IGNORED_FOLDERS.has(name) || name.startsWith('.')) continue
      const child = relative ? `${relative}/${name}` : name
      let isDir = false
      try {
        isDir = statSync(join(root, child)).isDirectory()
      } catch { /* a broken link */ }
      if (!isDir) continue
      if (existsSync(join(root, child, manifest))) found.push(child)
      walk(child, level + 1)
    }
  }
  walk('', 1)
  return found
}

function matching(folders: string[], patterns: string[]): string[] {
  const include = patterns.filter(pattern => !pattern.startsWith('!')).map(pattern => globToRegExp(pattern))
  const exclude = patterns.filter(pattern => pattern.startsWith('!')).map(pattern => globToRegExp(pattern.slice(1)))
  return folders.filter(folder => include.some(pattern => pattern.test(folder)) && !exclude.some(pattern => pattern.test(folder))).sort()
}

/** A TOML string list, enough of TOML for `members = ["crates/*", "app"]`. */
function tomlList(toml: string, section: string, key: string): string[] {
  const body = new RegExp(`^\\s*\\[${section}\\]([\\s\\S]*?)(?=^\\s*\\[|$(?![\\s\\S]))`, 'm').exec(toml)?.[1] ?? ''
  const list = new RegExp(`^\\s*${key}\\s*=\\s*\\[([\\s\\S]*?)\\]`, 'm').exec(body)?.[1] ?? ''
  return [...list.matchAll(/"([^"]+)"|'([^']+)'/g)].map(match => match[1] ?? match[2]!)
}

function cargoName(root: string, folder: string): string | null {
  try {
    return /^\s*\[package\][^[]*?^\s*name\s*=\s*"([^"]+)"/ms.exec(readFileSync(join(root, folder, 'Cargo.toml'), 'utf8'))?.[1] ?? null
  } catch {
    return null
  }
}

/** A Cargo workspace's member crates, from `[workspace] members` (and `exclude`) in the root Cargo.toml. */
function cargoMembers(root: string): string[] {
  let toml: string
  try {
    toml = readFileSync(join(root, 'Cargo.toml'), 'utf8')
  } catch {
    return []
  }
  const members = tomlList(toml, 'workspace', 'members')
  if (!members.length) return []
  const exclude = tomlList(toml, 'workspace', 'exclude').map(pattern => `!${pattern}`)
  return matching(packageFolders(root, 'Cargo.toml'), [...members, ...exclude])
}

/** Every Cargo.toml of the repository's crates: the root's when it is a crate itself, and the workspace members'. */
export function cargoManifests(root: string): string[] {
  const members = cargoMembers(root).map(folder => `${folder}/Cargo.toml`)
  return [...cargoName(root, '') ? ['Cargo.toml'] : [], ...members]
}

/**
 * The repository's workspace packages, sorted by folder; empty when it is not a workspace. JavaScript workspaces
 * (pnpm, npm, Yarn, Bun) and Cargo workspaces, whose crates are its packages.
 */
export function workspacePackages(root: string): WorkspacePackage[] {
  const patterns = workspacePatterns(root)
  const javascript = patterns.length
    ? matching(packageFolders(root), patterns).map((folder) => {
        const name = readJson(join(root, folder, 'package.json'))?.name
        return { folder, name: typeof name === 'string' && name ? name : folder }
      })
    : []
  const crates = cargoMembers(root).map(folder => ({ folder, name: cargoName(root, folder) ?? folder }))
  return [...javascript, ...crates.filter(crate => !javascript.some(item => item.folder === crate.folder))].sort((a, b) => a.folder.localeCompare(b.folder))
}
