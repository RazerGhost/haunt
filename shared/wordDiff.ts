/*
 * Which parts of a changed line changed: a removed line and the added line paired with it are split into words,
 * spaces and single marks, and what the longest common sequence of them leaves out is marked. Long lines are marked
 * whole: comparing them word by word would cost more than it shows.
 */

export interface Span { start: number, end: number }

const MAX_TOKENS = 300

function words(line: string): string[] {
  return line.match(/\w+|\s+|[^\w\s]/g) ?? []
}

/** The character spans of `before` and `after` that are not in both. */
export function changedSpans(before: string, after: string): { before: Span[], after: Span[] } {
  const a = words(before)
  const b = words(after)
  if (a.length > MAX_TOKENS || b.length > MAX_TOKENS) return { before: [{ start: 0, end: before.length }], after: [{ start: 0, end: after.length }] }
  // lengths[i][j]: the longest common sequence of a[i…] and b[j…].
  const lengths = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) lengths[i]![j] = a[i] === b[j] ? lengths[i + 1]![j + 1]! + 1 : Math.max(lengths[i + 1]![j]!, lengths[i]![j + 1]!)
  }
  const keepA = new Array<boolean>(a.length).fill(false)
  const keepB = new Array<boolean>(b.length).fill(false)
  for (let i = 0, j = 0; i < a.length && j < b.length;) {
    if (a[i] === b[j]) {
      keepA[i++] = true
      keepB[j++] = true
    } else if (lengths[i + 1]![j]! >= lengths[i]![j + 1]!) i++
    else j++
  }
  const spans = (tokens: string[], keep: boolean[]): Span[] => {
    const out: Span[] = []
    let at = 0
    tokens.forEach((token, index) => {
      if (!keep[index]) {
        const last = out.at(-1)
        if (last && last.end === at) last.end += token.length
        else out.push({ start: at, end: at + token.length })
      }
      at += token.length
    })
    return out
  }
  return { before: spans(a, keepA), after: spans(b, keepB) }
}

/** Splits styled pieces of a line at the spans' edges, marking the pieces inside a span. */
export function markSpans<T extends { content: string }>(pieces: T[], spans: Span[]): (T & { marked: boolean })[] {
  if (!spans.length) return pieces.map(piece => ({ ...piece, marked: false }))
  const out: (T & { marked: boolean })[] = []
  let at = 0
  for (const piece of pieces) {
    const end = at + piece.content.length
    const cuts = new Set([at, end])
    for (const span of spans) {
      if (span.start > at && span.start < end) cuts.add(span.start)
      if (span.end > at && span.end < end) cuts.add(span.end)
    }
    const points = [...cuts].sort((x, y) => x - y)
    for (let k = 0; k < points.length - 1; k++) {
      const from = points[k]!
      const to = points[k + 1]!
      const marked = spans.some(span => span.start <= from && to <= span.end)
      out.push({ ...piece, content: piece.content.slice(from - at, to - at), marked })
    }
    at = end
  }
  return out
}
