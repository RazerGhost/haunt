import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import type { Rule } from '../shared/types.ts'

/*
 * A project's own rules in code: the module `architecture.script` in haunt.config.json names. It runs in the scanner's
 * process, and only once you trust the project's folder. Its default export (or an export named `architecture`) is a
 * function, sync or async:
 *
 *   export default function architecture({ root, files }: { root: string, files: string[] }) {
 *     return {
 *       layers: [{ id: 'app' }, { id: 'domain' }],                 // top to bottom
 *       parts: [{ name: 'orders', layer: 'domain', role: '…', uses: [] }],
 *       partOf: (path: string) => …,                              // a file's part, or null: not in the rules
 *       judge: ({ part, path }, { part, path }) => ['layer'],      // what an import breaks, or []
 *       // or, instead of judge, its own judged imports and cycles:
 *       imports: [{ from, to, typeOnly, breaks: ['door'], known: true }],
 *       cycles: [['a.ts', 'b.ts']]
 *     }
 *   }
 *
 * Paths are relative to the project's root with forward slashes, as in `files` (what Haunt scans). With `judge`
 * (or neither), Haunt's own import graph is used; `known` marks breaks the project accepts for now (a baseline).
 */

export interface ScriptLayer { id: string, label?: string, note?: string }
export interface ScriptPart { name: string, layer: string, role?: string, uses?: string[], folder?: string }
export interface ScriptImport { from: string, to: string, typeOnly?: boolean, breaks?: Rule[], known?: boolean }
export interface ScriptArchitecture {
  layers: ScriptLayer[]
  parts: ScriptPart[]
  partOf: (path: string) => string | null | undefined
  judge?: (from: { part: string, path: string }, to: { part: string, path: string }) => Rule[]
  imports?: ScriptImport[]
  cycles?: string[][]
}

const RULES = new Set<Rule>(['layer', 'door'])

function fail(script: string, problem: string): never {
  throw new Error(`The architecture script ${script} ${problem}`)
}

/** Runs the script and checks what it returns; throws a sentence naming the problem. */
export async function runScript(root: string, script: string, files: string[]): Promise<ScriptArchitecture> {
  const file = join(root, script)
  if (!existsSync(file)) fail(script, 'does not exist.')
  let module: Record<string, unknown>
  try {
    module = await import(pathToFileURL(file).href) as Record<string, unknown>
  } catch (error) {
    fail(script, `could not be loaded: ${error instanceof Error ? error.message : error}`)
  }
  const entry = typeof module.default === 'function' ? module.default : module.architecture
  if (typeof entry !== 'function') fail(script, 'must export a function, as its default export or as "architecture".')
  let result: unknown
  try {
    result = await (entry as (context: { root: string, files: string[] }) => unknown)({ root, files: [...files] })
  } catch (error) {
    fail(script, `failed: ${error instanceof Error ? error.message : error}`)
  }
  if (!result || typeof result !== 'object') fail(script, 'must return an object with layers, parts and partOf.')
  const value = result as Partial<ScriptArchitecture>
  if (!Array.isArray(value.layers) || !value.layers.every(layer => layer && typeof layer.id === 'string')) fail(script, 'must return "layers": [{ id }], top to bottom.')
  const ids = new Set(value.layers.map(layer => layer.id))
  if (!Array.isArray(value.parts) || !value.parts.every(part => part && typeof part.name === 'string' && ids.has(part.layer))) {
    fail(script, 'must return "parts": [{ name, layer }], each in one of its layers.')
  }
  if (typeof value.partOf !== 'function') fail(script, 'must return "partOf(path)".')
  if (value.judge !== undefined && typeof value.judge !== 'function') fail(script, 'returned a "judge" that is not a function.')
  if (value.imports !== undefined) {
    if (!Array.isArray(value.imports) || !value.imports.every(edge => edge && typeof edge.from === 'string' && typeof edge.to === 'string' && (edge.breaks ?? []).every(rule => RULES.has(rule)))) {
      fail(script, 'returned "imports" that are not [{ from, to, typeOnly?, breaks?: ("layer" | "door")[], known? }].')
    }
  }
  if (value.cycles !== undefined && !(Array.isArray(value.cycles) && value.cycles.every(cycle => Array.isArray(cycle) && cycle.every(item => typeof item === 'string')))) {
    fail(script, 'returned "cycles" that are not lists of paths.')
  }
  return value as ScriptArchitecture
}
