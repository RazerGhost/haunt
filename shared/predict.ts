/*
 * Predict, on the Calls page's path view: the paths from the entry points down to a function, with the functions in
 * between covered by numbered places to fill in. You fill them from a list that mixes the real ones with decoys from
 * the same parts (or with any function you search for), then reveal the real paths, each place marked right or wrong.
 *
 * A pick is right when it is the function on the path, or when it fits there as well: the function above it on the
 * path calls it and it calls the function below. There can be more than one shortest way down, and finding another
 * one is knowing the code too.
 */

export interface Slot {
  /** `path:place`, unique in a puzzle. */
  id: string
  /** Which path, from 0, and the place in it (1 is the first after the entry point). */
  path: number
  at: number
  /** The function there. */
  real: string
  /** Its number on screen, from 1, counted along the paths. */
  number: number
}

export interface Puzzle {
  /** The paths, each from its entry point down to the function. */
  paths: string[][]
  slots: Slot[]
}

/**
 * The puzzle of `paths` (from callPaths' `pathsTo`): at most `max` of them that have functions between the entry point
 * and the function (a direct call has nothing to guess), the same middle only once.
 */
export function puzzle(paths: readonly string[][], max = 5): Puzzle {
  const kept: string[][] = []
  const seen = new Set<string>()
  for (const path of paths) {
    if (path.length < 3 || kept.length >= max) continue
    const middle = path.slice(1, -1).join('\n')
    if (seen.has(middle)) continue
    seen.add(middle)
    kept.push([...path])
  }
  const slots: Slot[] = []
  kept.forEach((path, index) => {
    for (let at = 1; at < path.length - 1; at++) slots.push({ id: `${index}:${at}`, path: index, at, real: path[at]!, number: slots.length + 1 })
  })
  return { paths: kept, slots }
}

/** A small seeded random (mulberry32): the same seed deals the same list, so it does not reshuffle on every change. */
export function seeded(seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6D2B79F5) >>> 0
    let x = state
    x = Math.imul(x ^ (x >>> 15), x | 1)
    x ^= x + Math.imul(x ^ (x >>> 7), x | 61)
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296
  }
}

function shuffled<T>(items: readonly T[], random: () => number): T[] {
  const out = [...items]
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1))
    const swap = out[i]!
    out[i] = out[j]!
    out[j] = swap
  }
  return out
}

/**
 * The list to pick from: the real functions once each, and as many decoys from `pool` (functions of the same parts, the
 * caller leaves out the entry points and the function itself) as there are real ones, at least four; dealt in a seeded
 * order.
 */
export function candidates(real: readonly string[], pool: readonly string[], seed: number): string[] {
  const random = seeded(seed)
  const unique = [...new Set(real)]
  const others = [...new Set(pool)].filter(id => !unique.includes(id))
  const decoys = shuffled(others, random).slice(0, Math.max(4, unique.length))
  return shuffled([...unique, ...decoys], random)
}

/** Each filled place, right or wrong; `calls(from, to)` says whether one function calls another. */
export function grade(found: Puzzle, picks: ReadonlyMap<string, string>, calls: (from: string, to: string) => boolean): Map<string, boolean> {
  const out = new Map<string, boolean>()
  for (const slot of found.slots) {
    const pick = picks.get(slot.id)
    if (!pick) continue
    const path = found.paths[slot.path]!
    out.set(slot.id, pick === slot.real || (calls(path[slot.at - 1]!, pick) && calls(pick, path[slot.at + 1]!)))
  }
  return out
}
