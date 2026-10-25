import type { FunctionNode, ScannedFile } from './types.ts'

/*
 * What looks unused, from how each function is used (the scan's `uses`: from another file, only its own, or not at all;
 * sidecar/calls.ts): functions nothing uses, likely dead, and exported functions used only in their own file, which need
 * not be exported. Left out, because something outside the scan likely uses them: test files, methods (called through
 * objects and interfaces), functions written into an object (`{ onSelect: () => … }`, a computed's `get`: whoever is
 * handed the object calls them), default exports (frameworks' conventions), files a framework or tool loads itself (pages,
 * layouts, plugins, middleware, server routes, config files), the exports of entry files (index, main, mod: a
 * package's face), and PHP, Rust and Lua, whose uses are not worked out.
 */

/**
 * `dead`: not exported, and nothing uses it. `unreferenced`: exported, and nothing in the scan uses it (something outside
 * may: a tool, another program). `export`: exported, but used only in its own file.
 */
export type UnusedKind = 'dead' | 'unreferenced' | 'export'

export interface Unused {
  id: string
  name: string
  path: string
  part: string
  line: number
  kind: UnusedKind
  exported: boolean
}

const TYPESCRIPT = /\.(?:[cm]?[jt]sx?|vue|svelte|astro)$/
/** Files a framework or tool loads by their place or name. */
const LOADED = /(?:^|\/)(?:pages|layouts|plugins|middleware|server|routes|app\/router)\/|(?:^|\/)(?:app|error)\.vue$|\.config\.[cm]?[jt]s$|(?:^|\/)(?:\+|\[)/
/** Entry files: what they export is a package's face. */
const ENTRY = /(?:^|\/)(?:index|main|mod)\.[cm]?[jt]sx?$/

export function unusedFunctions(files: ScannedFile[], uses: Record<string, 'outside' | 'inside' | 'held'>): Unused[] {
  const out: Unused[] = []
  for (const file of files) {
    if (file.test || !TYPESCRIPT.test(file.path) || LOADED.test(file.path)) continue
    const entry = ENTRY.test(file.path)
    const walk = (items: FunctionNode[]) => {
      for (const item of items) {
        walk(item.children)
        if (item.anonymous || item.test || item.kind === 'method' || item.name === 'default' || item.name === 'constructor') continue
        const use = uses[item.id]
        if (use === 'outside' || use === 'held') continue
        if (item.exported && entry) continue
        if (!use) out.push({ id: item.id, name: item.name, path: file.path, part: file.part, line: item.line, kind: item.exported ? 'unreferenced' : 'dead', exported: item.exported })
        else if (item.exported) out.push({ id: item.id, name: item.name, path: file.path, part: file.part, line: item.line, kind: 'export', exported: true })
      }
    }
    walk(file.functions)
  }
  return out.sort((a, b) => a.part.localeCompare(b.part) || a.path.localeCompare(b.path) || a.line - b.line)
}
