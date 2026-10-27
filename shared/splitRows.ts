/*
 * A hunk's lines side by side: a context line on both sides; within a change, each run of removed lines on the left
 * next to the run of added lines after it, row by row, with nothing on the shorter side.
 */

export interface SplitRow<T> { left: T | null, right: T | null }

export function splitRows<T extends { kind: 'add' | 'del' | 'context' }>(lines: T[]): SplitRow<T>[] {
  const rows: SplitRow<T>[] = []
  for (let at = 0; at < lines.length;) {
    const line = lines[at]!
    if (line.kind === 'context') {
      rows.push({ left: line, right: line })
      at++
      continue
    }
    const dels: T[] = []
    while (lines[at]?.kind === 'del') dels.push(lines[at++]!)
    const adds: T[] = []
    while (lines[at]?.kind === 'add') adds.push(lines[at++]!)
    for (let k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ left: dels[k] ?? null, right: adds[k] ?? null })
  }
  return rows
}
