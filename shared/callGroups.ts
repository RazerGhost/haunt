import { inside } from './folders.ts'
import { addTo } from './collect.ts'

/*
 * The columns of the call graph, grouped where they are long. A column with more than `GROUP_ABOVE` functions shows them
 * by part; a part the viewer opened (or the only part in its column) shows them by folder (the first two folders inside
 * the part: "features/audit/"); a folder the viewer opened shows its functions one by one. A group of one is shown as
 * its function. Nothing is left out.
 */

export const GROUP_ABOVE = 12

export interface Where {
  part: string
  /** The part's folder, so folders are named inside it. */
  partFolder: string
  path: string
}

export interface Shown {
  /** A function's id, or a group's ("group:<column>:<key>"). */
  id: string
  kind: 'function' | 'group'
  column: number
  /** For a group: its part, its folder ("" for a whole part) and its functions. */
  part?: string
  folder?: string
  members: string[]
  /** How many calls tie it to the column nearer the middle, for the order within a column. */
  weight: number
}

/** The folder a function is grouped under: the first two folders of its file inside its part ("./" at the top). */
export function groupFolder(where: Where): string {
  const folders = inside(where.path, where.partFolder).split('/').slice(0, -1)
  return folders.length ? `${folders.slice(0, 2).join('/')}/` : './'
}

/** The key of a part's group ("core:") or a folder's ("core:features/audit/"); the same in every column. */
function groupKey(part: string, folder = ''): string {
  return `${part}:${folder}`
}

/**
 * What each column shows. `columns` holds each function's column and its weight; `open` the group keys the viewer
 * opened; `keep` functions never folded away (the one in the middle, the picked one). Returns the shown items, and for
 * every function the id of what shows it (itself, or its group).
 */
export function groupColumns(
  columns: Map<string, { column: number, weight: number }>,
  whereOf: (id: string) => Where,
  open: Set<string>,
  keep: Set<string> = new Set()
): { shown: Shown[], shownAs: Map<string, string> } {
  const byColumn = new Map<number, string[]>()
  for (const [id, { column }] of columns) addTo(byColumn, column, id)
  const shown: Shown[] = []
  const shownAs = new Map<string, string>()
  const weightOf = (ids: string[]) => ids.reduce((sum, id) => sum + columns.get(id)!.weight, 0)
  const single = (id: string, column: number) => {
    shown.push({ id, kind: 'function', column, members: [id], weight: columns.get(id)!.weight })
    shownAs.set(id, id)
  }
  const group = (column: number, part: string, folder: string, members: string[]) => {
    const id = `group:${column}:${groupKey(part, folder)}`
    shown.push({ id, kind: 'group', column, part, folder, members, weight: weightOf(members) })
    for (const member of members) shownAs.set(member, id)
  }
  const bucket = (ids: string[], keyOf: (id: string) => string) => {
    const out = new Map<string, string[]>()
    for (const id of ids) addTo(out, keyOf(id), id)
    return out
  }
  for (const [column, ids] of byColumn) {
    if (ids.length <= GROUP_ABOVE) {
      for (const id of ids) single(id, column)
      continue
    }
    const loose = ids.filter(id => !keep.has(id))
    for (const id of ids) if (keep.has(id)) single(id, column)
    const parts = bucket(loose, id => whereOf(id).part)
    for (const [part, inPart] of parts) {
      if (inPart.length === 1) {
        single(inPart[0]!, column)
        continue
      }
      if (parts.size > 1 && !open.has(groupKey(part))) {
        group(column, part, '', inPart)
        continue
      }
      for (const [folder, inFolder] of bucket(inPart, id => groupFolder(whereOf(id)))) {
        if (inFolder.length === 1 || open.has(groupKey(part, folder))) {
          for (const id of inFolder) single(id, column)
        } else {
          group(column, part, folder, inFolder)
        }
      }
    }
  }
  return { shown, shownAs }
}
