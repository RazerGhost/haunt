import { existsSync, readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import type ts from 'typescript'
import type { Layer, PartImport } from '../shared/types.ts'
import type { Judge } from './imports.ts'
import { typescriptProject } from './projects/typescript.ts'
import { isSource, MAX_SOURCE_BYTES } from './languages.ts'
import { IGNORED_FOLDERS } from './workspaces.ts'

/*
 * What a scan needs to know about the repository: which files to read, where each one belongs (its part and the band
 * of the layer map), which are tests, and how to compile them for the call graph. Any TypeScript project works without
 * configuration (projects/typescript.ts); a project's own layers come from its haunt.config.json (config.ts), or from
 * the architecture script it names (script.ts).
 */

export interface Located {
  part: string
  /** The part's folder, for the header. */
  folder: string
  /** The band of the layer map the part is drawn in. */
  layer: string
}

export interface Project {
  kind: 'typescript'
  name: string
  /** The files to scan, relative to the repository with forward slashes, sorted. */
  files: string[]
  /** How the call graph's program compiles them. */
  options: ts.CompilerOptions
  /**
   * The declaration files the tsconfigs include (absolute, forward slashes), such as a framework's generated globals
   * (Nuxt's `.nuxt/imports.d.ts`): not scanned, but in the call graph's program, so the names they declare resolve.
   */
  ambient: string[]
  /** The bands of the layer map, top to bottom. */
  layers: Layer[]
  locate: (path: string) => Located
  isTest: (path: string) => boolean
  /** What a part is for, and the parts of its own layer it may use. */
  describe: (part: string) => { role: string, uses: string[] }
  /** Whose rules judge the imports: the project's haunt.config.json, the script it names, or none. */
  rules: 'config' | 'script' | null
  /** The imports and cycles, when the project's script works them out itself; otherwise the scan does. */
  ownImports: { imports: PartImport[], cycles: string[][] } | null
  /** How the scan's imports are judged, when they are judged. */
  judge: Judge | null
  /** Set when the project's layer rules could not be read. */
  architectureError: string | null
  /** Folders whose changes restart the scanner: the project's own rules in code, which Node loads once. */
  restartOn: string[]
  /** Folders to watch for changes, relative; the root itself is watched for its config files. */
  watch: string[]
}

export { isSource, isTestPath } from './languages.ts'

/** A path relative to the repository with forward slashes, or null when it is outside it or in an ignored folder. */
export function inRepository(root: string, file: string): string | null {
  const path = relative(root, file).split(sep).join('/')
  if (!path || path.startsWith('..') || path.split('/').some(segment => IGNORED_FOLDERS.has(segment))) return null
  return path
}

/**
 * Every source file under `dir` (relative to the repository) that `accept` takes (by default every language Haunt
 * reads), skipping dependencies, builds and files too large to have been written by hand.
 */
export function findSources(root: string, dir = '', accept: (path: string) => boolean = isSource): string[] {
  const out: string[] = []
  const walk = (relativeDir: string) => {
    const absolute = relativeDir ? join(root, relativeDir) : root
    if (!existsSync(absolute)) return
    for (const name of readdirSync(absolute)) {
      if (IGNORED_FOLDERS.has(name) || name.startsWith('.')) continue
      const path = relativeDir ? `${relativeDir}/${name}` : name
      let stats
      try {
        stats = statSync(join(root, path))
      } catch {
        continue // a broken link
      }
      if (stats.isDirectory()) walk(path)
      else if (accept(path) && stats.size <= MAX_SOURCE_BYTES) out.push(path)
    }
  }
  walk(dir)
  return out.sort()
}

/** What the scanner reads of `root`. Only a trusted folder's own code runs, such as its architecture script. */
export async function detectProject(root: string, typescript: typeof ts, trusted: boolean): Promise<Project> {
  return typescriptProject(root, typescript, trusted)
}
