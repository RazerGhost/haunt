import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { Rule } from '../shared/types.ts'
import type { Judge } from './imports.ts'
import { globToRegExp } from './workspaces.ts'

/*
 * A project's own layer map, in `haunt.config.json` at its root. All of it is optional; reading it runs no code.
 *
 *   {
 *     "layers": [{ "id": "app" }, { "id": "domain", "label": "Domain", "note": "the business rules" }, { "id": "infra" }],
 *     "parts": [
 *       { "match": "src/app", "layer": "app" },
 *       { "match": "src/domain/*", "name": "domain/$1", "layer": "domain", "uses": ["domain/shared"] },
 *       { "match": "src/infra/*", "layer": "infra" }
 *     ],
 *     "entries": ["index.ts"]
 *   }
 *
 * - `layers`, top to bottom: a part may use the parts of lower layers, and those of its own layer it lists in `uses`.
 * - `parts`: folder globs (`*` one folder, `**` any number); the first rule that matches a folder of a file makes that
 *   folder a part. `name` may use the globs' matches as `$1`, `$2`…; it defaults to the folder's name.
 * - `entries`: the files another part may import (front doors), relative to a part's folder.
 * Files no rule matches keep their part from the zero-config reading and are not judged.
 *
 * Or a project that has its own rules in code: `"architecture": { "script": "scripts/haunt.ts" }` names a module that
 * returns them (script.ts); it replaces `layers`, `parts` and `entries`, and only runs once you trust the folder.
 */

export interface LayerConfig { id: string, label?: string, note?: string }
export interface PartRule { match: string, name?: string, layer: string, uses?: string[], role?: string }
export interface HauntConfig {
  layers: LayerConfig[]
  parts: PartRule[]
  entries: string[]
  /** A module with the project's own rules, relative to the root. */
  architecture: { script: string } | null
}

const CONFIG_FILE = 'haunt.config.json'

const isText = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0
const isTextList = (value: unknown): value is string[] => Array.isArray(value) && value.every(isText)

/** The project's config, null when it has none; throws a sentence naming what is wrong. */
export function readConfig(root: string): HauntConfig | null {
  const file = join(root, CONFIG_FILE)
  if (!existsSync(file)) return null
  let raw: unknown
  try {
    raw = JSON.parse(readFileSync(file, 'utf8'))
  } catch (error) {
    throw new Error(`${CONFIG_FILE} is not valid JSON: ${error instanceof Error ? error.message : error}`)
  }
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error(`${CONFIG_FILE} must hold an object.`)
  const { layers = [], parts = [], entries = [] } = raw as Record<string, unknown>
  if (!Array.isArray(layers) || !layers.every(layer => layer && typeof layer === 'object' && isText((layer as LayerConfig).id))) {
    throw new Error(`${CONFIG_FILE}: "layers" must be a list of { "id": "…" }, top to bottom.`)
  }
  const ids = new Set((layers as LayerConfig[]).map(layer => layer.id))
  if (ids.size !== layers.length) throw new Error(`${CONFIG_FILE}: two layers have the same id.`)
  if (!Array.isArray(parts)) throw new Error(`${CONFIG_FILE}: "parts" must be a list of rules.`)
  parts.forEach((rule: PartRule, index) => {
    const where = `${CONFIG_FILE}: parts[${index}]`
    if (!rule || typeof rule !== 'object' || !isText(rule.match)) throw new Error(`${where} needs a "match" folder glob.`)
    if (!isText(rule.layer) || !ids.has(rule.layer)) throw new Error(`${where} names layer "${rule.layer}", which "layers" does not list.`)
    if (rule.uses !== undefined && !isTextList(rule.uses)) throw new Error(`${where}: "uses" must be a list of part names.`)
    if (rule.name !== undefined && !isText(rule.name)) throw new Error(`${where}: "name" must be text.`)
  })
  if (!isTextList(entries)) throw new Error(`${CONFIG_FILE}: "entries" must be a list of file names, e.g. ["index.ts"].`)
  const architecture = (raw as Record<string, unknown>).architecture
  let script: string | null = null
  if (architecture !== undefined) {
    const named = architecture && typeof architecture === 'object' ? (architecture as { script?: unknown }).script : undefined
    if (!isText(named)) throw new Error(`${CONFIG_FILE}: "architecture" must be { "script": "path/to/rules.ts" }.`)
    const normalised = named.replace(/\\/g, '/').replace(/^\.\//, '')
    // Inside the project, and a module Node can load itself.
    if (normalised.startsWith('/') || /^[a-z]:/i.test(normalised) || normalised.split('/').includes('..')) {
      throw new Error(`${CONFIG_FILE}: "architecture.script" must be a path inside the project.`)
    }
    if (!/\.(m?ts|m?js)$/.test(normalised)) throw new Error(`${CONFIG_FILE}: "architecture.script" must be a .ts, .mts, .js or .mjs file.`)
    script = normalised
  }
  return { layers: layers as LayerConfig[], parts: parts as PartRule[], entries, architecture: script ? { script } : null }
}

export interface ConfiguredPart { part: string, folder: string, layer: string, uses: string[], role: string }

/** Where a file belongs by the config's rules, or null when no rule matches any of its folders. */
export function configuredLocate(config: HauntConfig): (path: string) => ConfiguredPart | null {
  const rules = config.parts.map(rule => ({ rule, pattern: globToRegExp(rule.match, true) }))
  return (path) => {
    const folders = path.split('/').slice(0, -1)
    for (const { rule, pattern } of rules) {
      for (let depth = 1; depth <= folders.length; depth++) {
        const folder = folders.slice(0, depth).join('/')
        const match = pattern.exec(folder)
        if (!match) continue
        const part = rule.name ? rule.name.replace(/\$(\d)/g, (_all, group: string) => match[Number(group)] ?? '') : folders[depth - 1]!
        return { part, folder, layer: rule.layer, uses: rule.uses ?? [], role: rule.role ?? '' }
      }
    }
    return null
  }
}

/**
 * The config's rules as a judge of imports between parts: a lower layer, or a part of the same layer the importing
 * part lists in `uses` (`layer`); and, with `entries`, only through one of them (`door`). Imports from or into parts
 * no rule made are not judged.
 */
export function configuredJudge(config: HauntConfig, locate: (path: string) => ConfiguredPart | null): Judge {
  const rank = new Map(config.layers.map((layer, index) => [layer.id, index]))
  return (from, to) => {
    const a = locate(from.path), b = locate(to.path)
    if (!a || !b) return []
    const breaks: Rule[] = []
    const lower = rank.get(b.layer)! > rank.get(a.layer)!
    const allowed = lower || (a.layer === b.layer && a.uses.includes(b.part))
    if (!allowed) breaks.push('layer')
    if (config.entries.length && !config.entries.some(entry => to.path === `${b.folder}/${entry}`)) breaks.push('door')
    return breaks
  }
}
