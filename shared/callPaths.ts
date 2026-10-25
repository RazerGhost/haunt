/*
 * How a function is reached, for the Calls page's path view: the shortest call paths from the code's entry points (the
 * functions nothing in the scan calls: route handlers, exported APIs, scripts' mains) down to it, and what it calls
 * below it, a row per step. Rows are ordered so the lines between them cross as little as they can (each function sits
 * under the average place of the ones it is called from).
 */
import { addTo } from './collect.ts'

export interface Edge { from: string, to: string }

export interface PathRow {
  /** Steps from the function: negative above it (callers), 0 the function, positive below it (what it calls). */
  step: number
  ids: string[]
}

export interface PathView {
  rows: PathRow[]
  /** The edges between the functions shown. */
  edges: Edge[]
  /** The entry points the paths start from. */
  entries: string[]
  /** Paths found (at most `limit`), and the shortest and longest of them in steps. */
  paths: number
  shortest: number
  longest: number
  /** Nothing in the scan calls it: it is an entry point itself. */
  isEntry: boolean
  /** Its callers never reach an entry point (only cycles above it): the rows show the callers found instead. */
  cyclic: boolean
}

/**
 * The shortest paths, at most `limit`, from entry points to `target`, each from the entry point down to `target`.
 * `callers(id)` gives the functions that call `id`. An entry point is a function without callers; `keep` leaves out
 * functions that are not to be shown (tests), as if they did not call.
 */
export function pathsTo(target: string, callers: (id: string) => string[], limit = 12, keep: (id: string) => boolean = () => true): string[][] {
  // Breadth first upwards: each function's distance from the target, and the ones it was reached from.
  const distance = new Map<string, number>([[target, 0]])
  const below = new Map<string, string[]>()
  let frontier = [target]
  const entries: string[] = []
  while (frontier.length && entries.length < limit) {
    const next: string[] = []
    for (const id of frontier) {
      const up = callers(id).filter(keep)
      if (!up.length && id !== target) entries.push(id)
      for (const caller of up) {
        if (!distance.has(caller)) {
          distance.set(caller, distance.get(id)! + 1)
          next.push(caller)
        }
        // Every shortest way down from the caller: through functions one step nearer the target.
        if (distance.get(caller) === distance.get(id)! + 1) addTo(below, caller, id)
      }
    }
    frontier = next
  }
  // From each entry point, down the shortest ways to the target, until `limit` paths.
  const paths: string[][] = []
  const walk = (id: string, path: string[]) => {
    if (paths.length >= limit) return
    if (id === target) {
      paths.push(path)
      return
    }
    for (const next of below.get(id) ?? []) walk(next, [...path, next])
  }
  for (const entry of entries) walk(entry, [entry])
  return paths
}

/** The path view of `target`: the paths from entry points above it, and `down` steps of what it calls below it. */
export function pathView(
  target: string,
  callers: (id: string) => string[],
  callees: (id: string) => string[],
  { limit = 12, down = 1, perRow = 30, keep = () => true }: { limit?: number, down?: number, perRow?: number, keep?: (id: string) => boolean } = {}
): PathView {
  const isEntry = callers(target).filter(keep).length === 0
  let paths = isEntry ? [] : pathsTo(target, callers, limit, keep)
  let cyclic = false
  if (!isEntry && !paths.length) {
    // Only cycles above: show the direct callers instead, so the view is not empty.
    cyclic = true
    paths = callers(target).filter(keep).slice(0, limit).map(caller => [caller, target])
  }
  const step = new Map<string, number>([[target, 0]])
  for (const path of paths) {
    path.forEach((id, at) => {
      const s = at - (path.length - 1)
      // A function on several paths sits at its nearest step.
      if (!step.has(id) || s > step.get(id)!) step.set(id, s)
    })
  }
  const edges: Edge[] = []
  const seen = new Set<string>()
  const addEdge = (from: string, to: string) => {
    const key = `${from}\n${to}`
    if (!seen.has(key)) {
      seen.add(key)
      edges.push({ from, to })
    }
  }
  for (const path of paths) for (let at = 1; at < path.length; at++) addEdge(path[at - 1]!, path[at]!)
  // Below: what it calls, a row per step, each function once.
  let frontier = [target]
  for (let s = 1; s <= down; s++) {
    const next: string[] = []
    for (const id of frontier) {
      for (const callee of callees(id).filter(keep)) {
        if (!step.has(callee)) {
          if (next.length >= perRow) continue
          step.set(callee, s)
          next.push(callee)
        }
        if (step.get(callee) === s) addEdge(id, callee)
      }
    }
    frontier = next
  }
  // Rows, ordered from the function outwards: each one under (or over) the average place of its neighbours nearer it.
  const rows = new Map<number, string[]>()
  for (const [id, s] of step) addTo(rows, s, id)
  const place = new Map<string, number>([[target, 0]])
  const order = (s: number, nearer: number) => {
    const ids = rows.get(s)
    if (!ids) return
    const average = (id: string) => {
      const near = edges.filter(edge => (edge.from === id && step.get(edge.to) === nearer) || (edge.to === id && step.get(edge.from) === nearer))
        .map(edge => place.get(edge.from === id ? edge.to : edge.from) ?? 0)
      return near.length ? near.reduce((a, b) => a + b, 0) / near.length : 0
    }
    const sorted = [...ids].sort((a, b) => average(a) - average(b) || a.localeCompare(b))
    sorted.forEach((id, at) => place.set(id, at - (sorted.length - 1) / 2))
    rows.set(s, sorted)
  }
  const steps = [...rows.keys()]
  for (let s = -1; s >= Math.min(...steps); s--) order(s, s + 1)
  for (let s = 1; s <= Math.max(...steps); s++) order(s, s - 1)
  const lengths = paths.map(path => path.length - 1)
  return {
    rows: [...rows].sort((a, b) => a[0] - b[0]).map(([s, ids]) => ({ step: s, ids })),
    edges,
    entries: [...new Set(paths.map(path => path[0]!))].filter(id => id !== target && !cyclic),
    paths: cyclic ? 0 : paths.length,
    shortest: lengths.length && !cyclic ? Math.min(...lengths) : 0,
    longest: lengths.length && !cyclic ? Math.max(...lengths) : 0,
    isEntry,
    cyclic
  }
}

/** The entry points of the scan: functions with callees but no callers, most calling first. */
export function entryPoints(ids: string[], callers: (id: string) => string[], callees: (id: string) => string[]): string[] {
  return ids
    .filter(id => callers(id).length === 0 && callees(id).length > 0)
    .sort((a, b) => callees(b).length - callees(a).length || a.localeCompare(b))
}
