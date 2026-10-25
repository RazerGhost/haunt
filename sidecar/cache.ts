import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs'
import { isAbsolute, join } from 'node:path'
import type { CallEdge } from '../shared/types.ts'
import type { Uses } from './calls.ts'

/*
 * The call graph kept between launches. Working it out takes the type checker over every file (half a minute on a big
 * repository), so each repository's graph is saved in the app's cache folder (HAUNT_CACHE), with a fingerprint of what
 * it was worked out from: every scanned file's size and time, the files that change how they compile or resolve
 * (tsconfig, package and lock files, haunt.config.json), whether the folder is trusted, and the format's version. At
 * the next launch, a matching fingerprint means the saved graph is the one the scan would work out.
 */

/** Raised when how calls are worked out changes (sidecar/calls.ts, treesitter's matching), so old graphs are not used. */
const FORMAT = 'calls-2'

/** At the repository's root: what changes how its files compile and import, beside the files themselves. */
const SETTINGS = ['tsconfig.json', 'jsconfig.json', 'package.json', 'pnpm-lock.yaml', 'package-lock.json', 'yarn.lock', 'bun.lock', 'bun.lockb', 'composer.json', 'composer.lock', 'Cargo.toml', 'Cargo.lock', 'haunt.config.json']

function stamp(file: string): string {
  try {
    const stats = statSync(file)
    return `${stats.size}:${Math.round(stats.mtimeMs)}`
  } catch {
    return '-'
  }
}

/**
 * The fingerprint of the files a call graph is worked out from: the root's settings, `settings` (each workspace part's
 * own tsconfig and package.json, the declaration files a framework generates), and the files themselves.
 */
export function fingerprint(root: string, paths: string[], trusted: boolean, settings: string[] = []): string {
  const hash = createHash('sha256')
  hash.update(`${FORMAT}\n${trusted ? 'trusted' : 'untrusted'}\n`)
  for (const name of SETTINGS) hash.update(`${name} ${stamp(join(root, name))}\n`)
  for (const path of [...new Set(settings)].sort()) hash.update(`setting ${path} ${stamp(isAbsolute(path) ? path : join(root, path))}\n`)
  for (const path of [...paths].sort()) hash.update(`${path} ${stamp(join(root, path))}\n`)
  return hash.digest('hex')
}

function fileOf(folder: string, root: string): string {
  return join(folder, `${createHash('sha256').update(root).digest('hex').slice(0, 24)}.json`)
}

/** The saved call graph (and functions' uses) of `root`, if it was worked out from what `key` fingerprints. */
export function readCalls(folder: string | undefined, root: string, key: string): { calls: CallEdge[], uses: Uses } | null {
  if (!folder) return null
  try {
    const saved = JSON.parse(readFileSync(fileOf(folder, root), 'utf8')) as { key?: string, calls?: CallEdge[], uses?: Uses }
    return saved.key === key && Array.isArray(saved.calls) ? { calls: saved.calls, uses: saved.uses ?? {} } : null
  } catch {
    return null
  }
}

/** Saves `root`'s call graph with its fingerprint; a failure only means it is worked out again next time. */
export function writeCalls(folder: string | undefined, root: string, key: string, graph: { calls: CallEdge[], uses: Uses }): void {
  if (!folder) return
  try {
    if (!existsSync(folder)) mkdirSync(folder, { recursive: true })
    const file = fileOf(folder, root)
    // Written next to it, then moved in place: a launch never reads half a file.
    writeFileSync(`${file}.tmp`, JSON.stringify({ key, ...graph }))
    renameSync(`${file}.tmp`, file)
  } catch { /* not kept */ }
}
