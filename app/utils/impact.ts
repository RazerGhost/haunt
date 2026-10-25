import type { ChangedFile } from '~~/shared/types'
import type { ScanIndex } from '~/composables/useCode'

/*
 * What a change could break: the functions that reach a changed one through calls, a few steps out, in the call graph
 * of the code as it is now. Shared by the local changes and the commit list's lenses.
 */

export interface Affected {
  /** Steps from the nearest changed function: 1 calls it directly. */
  distance: number
  /** The changed function it reaches. */
  via: string
}

/** The functions that reach one of `changed` through calls, up to `depth` steps, nearest first. */
export function blastRadius(index: ScanIndex | null, changed: Set<string>, depth: number): Map<string, Affected> {
  const out = new Map<string, Affected>()
  if (!index) return out
  let frontier = [...changed].map(id => ({ id, via: id }))
  for (let distance = 1; distance <= depth && frontier.length; distance++) {
    const next: { id: string, via: string }[] = []
    for (const { id, via } of frontier) {
      for (const edge of index.callers.get(id) ?? []) {
        if (changed.has(edge.from) || out.has(edge.from)) continue
        out.set(edge.from, { distance, via })
        next.push({ id: edge.from, via })
      }
    }
    frontier = next
  }
  return out
}

/** Only the affected functions that have a name: anonymous callbacks carry the walk on but are not listed or counted. */
export function namedOnly(index: ScanIndex | null, affected: Map<string, Affected>): Map<string, Affected> {
  return new Map([...affected].filter(([id]) => index?.functions.get(id)?.fn.anonymous === false))
}

/** Per part: how many functions changed (not counting removed ones) and how many named ones call a changed one. */
export function countsByPart(index: ScanIndex | null, files: ChangedFile[], affected: Map<string, Affected>): Map<string, { changed: number, affected: number }> {
  const parts = new Map<string, { changed: number, affected: number }>()
  const at = (part: string) => parts.get(part) ?? parts.set(part, { changed: 0, affected: 0 }).get(part)!
  for (const file of files) if (file.part) at(file.part).changed += file.functions.filter(fn => fn.status !== 'removed').length
  for (const id of affected.keys()) {
    const part = index?.functions.get(id)?.file.part
    if (part) at(part).affected++
  }
  return parts
}

/** The ids, in the scan, of the functions a change added or changed. */
export function changedIds(files: ChangedFile[]): Set<string> {
  return new Set(files.flatMap(file => file.functions.filter(fn => fn.id && fn.status !== 'removed').map(fn => fn.id!)))
}
