import type { FunctionNode, InnerImport, ScannedFile } from './types.ts'
import { addTo } from './collect.ts'

/*
 * Inside a part, folder by folder: the folder tree of a part's files, and the imports between the folders and files
 * one level inside a folder, added up, with the cycles among them. Paths inside a part are relative to its folder, a
 * folder's with a trailing slash ("features/audit/"); the part's own folder is "".
 */

export interface Folder {
  /** Relative to the part's folder, with a trailing slash; "" for the part's own folder. */
  path: string
  name: string
  folders: Folder[]
  /** The files directly in it. */
  files: ScannedFile[]
  /** Counted through every folder inside it. */
  fileCount: number
  functionCount: number
  lines: number
}

function count(items: FunctionNode[]): number {
  return items.reduce((sum, fn) => sum + 1 + count(fn.children), 0)
}

/** A path inside the part: relative to its folder. */
export function inside(path: string, partFolder: string): string {
  const folder = partFolder === '.' ? '' : partFolder
  return folder && path.startsWith(`${folder}/`) ? path.slice(folder.length + 1) : path
}

/** The folder tree of a part's files. */
export function folderTree(files: ScannedFile[], partFolder: string): Folder {
  const root: Folder = { path: '', name: '', folders: [], files: [], fileCount: 0, functionCount: 0, lines: 0 }
  for (const file of files) {
    const parts = inside(file.path, partFolder).split('/')
    parts.pop()
    let at = root
    const functions = count(file.functions)
    for (const segment of [null, ...parts]) {
      if (segment !== null) {
        const path = `${at.path}${segment}/`
        let next = at.folders.find(folder => folder.path === path)
        if (!next) {
          next = { path, name: segment, folders: [], files: [], fileCount: 0, functionCount: 0, lines: 0 }
          at.folders.push(next)
        }
        at = next
      }
      at.fileCount++
      at.functionCount += functions
      at.lines += file.lines
    }
    at.files.push(file)
  }
  const sort = (folder: Folder) => {
    folder.folders.sort((a, b) => a.name.localeCompare(b.name))
    folder.files.sort((a, b) => a.path.localeCompare(b.path))
    folder.folders.forEach(sort)
  }
  sort(root)
  return root
}

/** The folder at `path` in the tree, or null. */
export function folderAt(root: Folder, path: string): Folder | null {
  if (!path) return root
  let at: Folder | undefined = root
  for (const segment of path.split('/').filter(Boolean)) {
    at = at?.folders.find(folder => folder.name === segment)
    if (!at) return null
  }
  return at ?? null
}

export interface LevelNode {
  /** A subfolder's path ("features/audit/") or a file's path in the repository. */
  id: string
  kind: 'folder' | 'file'
  label: string
  files: number
  functions: number
}

export interface LevelEdge {
  from: string
  to: string
  /** How many file imports it adds up. */
  count: number
  /** Every one of them imports types only. */
  typeOnly: boolean
  /** It is part of a cycle among the nodes (of runtime imports). */
  cycle: boolean
}

/** The node a file belongs to one level inside `folder` (a subfolder's path, or its own), or null outside the folder. */
export function nodeAtLevel(folder: Folder, partFolder: string): (path: string) => string | null {
  const direct = new Set(folder.files.map(file => file.path))
  return (path) => {
    if (direct.has(path)) return path
    const relative = inside(path, partFolder)
    if (!relative.startsWith(folder.path)) return null
    const rest = relative.slice(folder.path.length)
    const slash = rest.indexOf('/')
    return slash < 0 ? null : `${folder.path}${rest.slice(0, slash + 1)}`
  }
}

/** One level inside `folder`: its subfolders and files as nodes, and the imports between them, added up. */
export function level(folder: Folder, imports: InnerImport[], partFolder: string): { nodes: LevelNode[], edges: LevelEdge[] } {
  const nodes: LevelNode[] = [
    ...folder.folders.map(sub => ({ id: sub.path, kind: 'folder' as const, label: sub.name, files: sub.fileCount, functions: sub.functionCount })),
    ...folder.files.map(file => ({ id: file.path, kind: 'file' as const, label: file.path.split('/').pop()!, files: 1, functions: count(file.functions) }))
  ]
  const nodeOf = nodeAtLevel(folder, partFolder)
  const edges = new Map<string, LevelEdge>()
  for (const item of imports) {
    const from = nodeOf(item.from)
    const to = nodeOf(item.to)
    if (!from || !to || from === to) continue
    const key = `${from}\n${to}`
    const edge = edges.get(key) ?? { from, to, count: 0, typeOnly: true, cycle: false }
    edge.count++
    edge.typeOnly &&= item.typeOnly
    edges.set(key, edge)
  }
  const list = [...edges.values()]
  const cyclic = cycles(nodes.map(node => node.id), list.filter(edge => !edge.typeOnly))
  for (const edge of list) edge.cycle = !edge.typeOnly && cyclic.some(group => group.has(edge.from) && group.has(edge.to))
  return { nodes, edges: list }
}

/** The groups of two or more nodes that reach each other (strongly connected components). */
export function cycles(ids: string[], edges: { from: string, to: string }[]): Set<string>[] {
  const next = new Map<string, string[]>()
  for (const edge of edges) addTo(next, edge.from, edge.to)
  const index = new Map<string, number>()
  const low = new Map<string, number>()
  const stack: string[] = []
  const on = new Set<string>()
  const out: Set<string>[] = []
  let counter = 0
  const visit = (id: string) => {
    index.set(id, counter)
    low.set(id, counter)
    counter++
    stack.push(id)
    on.add(id)
    for (const to of next.get(id) ?? []) {
      if (!index.has(to)) {
        visit(to)
        low.set(id, Math.min(low.get(id)!, low.get(to)!))
      } else if (on.has(to)) {
        low.set(id, Math.min(low.get(id)!, index.get(to)!))
      }
    }
    if (low.get(id) === index.get(id)) {
      const group = new Set<string>()
      let member: string
      do {
        member = stack.pop()!
        on.delete(member)
        group.add(member)
      } while (member !== id)
      if (group.size > 1) out.push(group)
    }
  }
  for (const id of ids) if (!index.has(id)) visit(id)
  return out
}
