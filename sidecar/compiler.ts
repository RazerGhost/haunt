import { createRequire } from 'node:module'
import { join } from 'node:path'

/*
 * Whether a project's own TypeScript can read it: the scan calls TypeScript's compiler API (tsconfig files, programs,
 * syntax trees), and TypeScript 7, the native compiler, ships none of it (`require('typescript')` gives only its
 * version; the rest is under experimental `typescript/unstable/*` entry points). A TypeScript without it is left
 * aside and Haunt's own reads the project (main.ts), which then says so.
 */

/** What the scan calls first; a TypeScript that has these has the rest of the API it uses. */
const NEEDED = ['getParsedCommandLineOfConfigFile', 'createProgram', 'createCompilerHost', 'createSourceFile', 'forEachChild'] as const

export interface OwnTypescript {
  /** Its file, as the scan imports it. */
  path: string
  version: string
  /** The API calls it lacks; empty when it can read the project. */
  missing: string[]
}

/** The TypeScript installed in `root` (loading it runs its code: only for a trusted folder), or null when it has none. */
export function ownTypescript(root: string): OwnTypescript | null {
  const require = createRequire(join(root, 'package.json'))
  let path: string
  try {
    path = require.resolve('typescript')
  } catch {
    return null
  }
  let loaded: Record<string, unknown>
  try {
    loaded = require(path) as Record<string, unknown>
  } catch {
    return { path, version: 'unknown', missing: [...NEEDED] }
  }
  const version = typeof loaded.version === 'string' ? loaded.version : 'unknown'
  return { path, version, missing: NEEDED.filter(name => typeof loaded[name] !== 'function') }
}
