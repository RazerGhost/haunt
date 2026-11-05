/*
 * What you know of the code: the functions you marked as understood, or showed you knew (a route's chain recalled, a
 * call path predicted). Kept on this machine per repository (useFamiliarity), never in the repository.
 *
 * A record names its function by file and qualified name (`Order.store`, or `outer>inner` for a function written in
 * another), since its id holds its line, which moves as the code above it is edited. It keeps a hash of the function's
 * code as it was when you learned it: when the code is different now, the record is stale, not familiar. The hash
 * leaves out indentation and blank lines, so moving or reformatting code keeps what you knew.
 */

export type FamiliarSource = 'route-recall' | 'call-predict' | 'manual'
export const FAMILIAR_SOURCES: readonly FamiliarSource[] = ['route-recall', 'call-predict', 'manual']

export interface FamiliarRecord {
  /** When you learned it, in milliseconds since 1970. */
  at: number
  source: FamiliarSource
  /** The hash of its code then (hashCode). */
  hash: string
}

/** Familiar: learned and unchanged since. Stale: learned, but its code is different now. */
export type Familiarity = 'familiar' | 'stale'

/** A function's name for the store: its file and its name, after the names of the functions it is written in. */
export function familiarKey(path: string, names: readonly string[]): string {
  return `${path}#${names.join('>')}`
}

/**
 * A hash of a function's code: its lines without their indentation or trailing space, blank lines left out, and how
 * many lines it has (a function read only in part is still told apart from a longer one). cyrb53: 53 bits, quick, not
 * for security.
 */
export function hashCode(lines: readonly string[], length = lines.length): string {
  const text = `${length}\n${lines.map(line => line.trim()).filter(Boolean).join('\n')}`
  let h1 = 0xDEADBEEF
  let h2 = 0x41C6CE57
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    h1 = Math.imul(h1 ^ code, 2654435761)
    h2 = Math.imul(h2 ^ code, 1597334677)
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909)
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909)
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36)
}

/**
 * Where a function stands: null when you never learned it; familiar when its code is as it was; stale when it changed.
 * A hash not read yet (`now` undefined) keeps it familiar until it is.
 */
export function familiarity(record: FamiliarRecord | undefined, now: string | undefined): Familiarity | null {
  if (!record) return null
  return now === undefined || now === record.hash ? 'familiar' : 'stale'
}

/** Whether a stored value is a repository's records, to read the store back safely. */
export function isRecords(value: unknown): value is Record<string, FamiliarRecord> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  return Object.values(value).every(item => !!item && typeof item === 'object'
    && typeof (item as FamiliarRecord).at === 'number'
    && typeof (item as FamiliarRecord).hash === 'string'
    && FAMILIAR_SOURCES.includes((item as FamiliarRecord).source))
}

/** How much of a set of functions you know: how many there are, how many are familiar, how many went stale. */
export interface Share {
  total: number
  familiar: number
  stale: number
}

export function tally(ids: Iterable<string>, states: ReadonlyMap<string, Familiarity>): Share {
  const share: Share = { total: 0, familiar: 0, stale: 0 }
  for (const id of ids) {
    share.total++
    const state = states.get(id)
    if (state) share[state]++
  }
  return share
}

/**
 * The share of every box of a tree of boxes (the canvas: parts, folders, files, functions in functions): each box counts
 * the functions in it, a function itself and the functions written in it.
 */
export function tallyTree<T extends { id: string, kind: string, children: readonly T[] }>(root: T, states: ReadonlyMap<string, Familiarity>): Map<string, Share> {
  const out = new Map<string, Share>()
  const walk = (box: T): Share => {
    const share: Share = { total: 0, familiar: 0, stale: 0 }
    if (box.kind === 'function') {
      share.total++
      const state = states.get(box.id)
      if (state) share[state]++
    }
    for (const child of box.children) {
      const inner = walk(child)
      share.total += inner.total
      share.familiar += inner.familiar
      share.stale += inner.stale
    }
    out.set(box.id, share)
    return share
  }
  walk(root)
  return out
}

/** The familiar part of a share, in whole percents. */
export function percent(share: Share): number {
  return share.total ? Math.round(share.familiar / share.total * 100) : 0
}
