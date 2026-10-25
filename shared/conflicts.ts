/*
 * A conflicted file as git leaves it: plain text, and blocks between git's markers with your lines and theirs (and the
 * common base's, in the diff3 style). The conflict view shows each block with a choice (yours, theirs, both) and
 * writes the file back from the choices.
 *
 *   <<<<<<< ours
 *   your lines
 *   ||||||| base          (diff3 only)
 *   the base's lines
 *   =======
 *   their lines
 *   >>>>>>> theirs
 */

export type Segment =
  | { kind: 'text', text: string }
  | { kind: 'conflict', ours: string, base: string | null, theirs: string, oursLabel: string, baseLabel: string, theirsLabel: string }

export type Choice = 'ours' | 'theirs' | 'both'

const START = '<<<<<<<'
const BASE = '|||||||'
const MIDDLE = '======='
const END = '>>>>>>>'

const isMarker = (line: string, marker: string) => line === marker || line.startsWith(`${marker} `)

/** The file's text split into plain text and conflict blocks; line endings are kept as they are. */
export function parseConflicts(text: string): Segment[] {
  const lines = text.match(/[^\n]*\n|[^\n]+$/g) ?? []
  const out: Segment[] = []
  let plain = ''
  let at = 0
  while (at < lines.length) {
    const line = lines[at]!
    const bare = line.replace(/\r?\n$/, '')
    if (!isMarker(bare, START)) {
      plain += line
      at++
      continue
    }
    // A block: read up to its end marker; one without an end is plain text after all.
    let ours = ''
    let base: string | null = null
    let theirs = ''
    let part: 'ours' | 'base' | 'theirs' = 'ours'
    let theirsLabel = ''
    let baseLabel = ''
    let end = -1
    for (let next = at + 1; next < lines.length; next++) {
      const current = lines[next]!
      const trimmed = current.replace(/\r?\n$/, '')
      if (part === 'ours' && isMarker(trimmed, BASE)) {
        part = 'base'
        base = ''
        baseLabel = trimmed.slice(BASE.length).trim()
      } else if (part !== 'theirs' && isMarker(trimmed, MIDDLE)) {
        part = 'theirs'
      } else if (part === 'theirs' && isMarker(trimmed, END)) {
        theirsLabel = trimmed.slice(END.length).trim()
        end = next
        break
      } else if (part === 'ours') {
        ours += current
      } else if (part === 'base') {
        base += current
      } else {
        theirs += current
      }
    }
    if (end < 0) {
      plain += line
      at++
      continue
    }
    if (plain) out.push({ kind: 'text', text: plain })
    plain = ''
    out.push({ kind: 'conflict', ours, base, theirs, oursLabel: bare.slice(START.length).trim(), baseLabel, theirsLabel })
    at = end + 1
  }
  if (plain) out.push({ kind: 'text', text: plain })
  return out
}

/** The text with block `index` (counting conflict blocks only) replaced by the chosen lines. */
export function choose(segments: Segment[], index: number, choice: Choice): Segment[] {
  let count = -1
  return segments.map((segment) => {
    if (segment.kind !== 'conflict' || ++count !== index) return segment
    const text = choice === 'ours' ? segment.ours : choice === 'theirs' ? segment.theirs : segment.ours + segment.theirs
    return { kind: 'text', text }
  })
}

/** The file written back from its segments; blocks not yet chosen keep their markers. */
export function joinSegments(segments: Segment[]): string {
  return segments.map((segment) => {
    if (segment.kind === 'text') return segment.text
    const nl = /\r\n/.test(segment.ours + segment.theirs) ? '\r\n' : '\n'
    const head = `${START}${segment.oursLabel ? ` ${segment.oursLabel}` : ''}${nl}`
    const base = segment.base === null ? '' : `${BASE}${segment.baseLabel ? ` ${segment.baseLabel}` : ''}${nl}${segment.base}`
    return `${head}${segment.ours}${base}${MIDDLE}${nl}${segment.theirs}${END}${segment.theirsLabel ? ` ${segment.theirsLabel}` : ''}${nl}`
  }).join('')
}

/** How many conflict blocks are left. */
export function conflictsLeft(segments: Segment[]): number {
  return segments.filter(segment => segment.kind === 'conflict').length
}
