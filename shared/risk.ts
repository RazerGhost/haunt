/*
 * The risk of a change in one line, from what Haunt measures already: how many functions call a changed one (two steps
 * out), how many changed functions no test reaches, how many busy files it touches (changed often lately), and how big
 * it is. Each counts for points; the points give low, medium or high, and the reasons say why in words. A guide for
 * where to look first, not a verdict.
 */

export interface RiskInput {
  /** Changed functions still in the code (added, changed or moved). */
  changed: number
  /** Of those, how many no test reaches, among the ones whose tests Haunt reads (TypeScript and JavaScript). */
  unreached: number
  /** Of those, how many are in a language whose tests Haunt does not read (PHP, Rust, Lua): left out of `unreached`. */
  unread?: number
  /** Named functions that call a changed one, up to two steps out. */
  callers: number
  /** The files it touches that changed often lately, busiest first. */
  busy: { path: string, commits: number }[]
  /** How many days "lately" is. */
  days: number
}

export type RiskLevel = 'low' | 'medium' | 'high'

export interface Risk {
  level: RiskLevel
  points: number
  reasons: string[]
}

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`

export function riskOf(input: RiskInput): Risk {
  let points = 0
  const reasons: string[] = []
  if (input.callers >= 20) points += 2
  else if (input.callers >= 5) points += 1
  if (input.callers) reasons.push(`${plural(input.callers, 'function calls', 'functions call')} what changed`)

  const read = input.changed - (input.unread ?? 0)
  if (input.unreached) {
    points += input.unreached * 2 >= read ? 2 : 1
    reasons.push(`${input.unreached} of ${plural(read, 'changed function', 'changed functions')} no test reaches`)
  } else if (read) {
    reasons.push(read === 1 ? 'a test reaches the changed function' : `tests reach every changed function${input.unread ? ' Haunt reads tests for' : ''}`)
  }
  if (input.unread) reasons.push(`tests not read for ${input.unread} (PHP, Rust or Lua)`)

  if (input.busy.length) {
    points += input.busy.length >= 3 ? 2 : 1
    const first = input.busy[0]!
    const name = first.path.split('/').pop()
    reasons.push(input.busy.length === 1
      ? `touches a busy file (${name}, ${first.commits} commits in ${input.days} days)`
      : `touches ${input.busy.length} busy files (the busiest ${name}, ${first.commits} commits in ${input.days} days)`)
  }

  if (input.changed >= 15) {
    points += 1
    reasons.push(`${input.changed} functions changed`)
  }

  const level: RiskLevel = points >= 4 ? 'high' : points >= 2 ? 'medium' : 'low'
  return { level, points, reasons }
}

/** The files that count as busy: changed at least eight times in the period and among the busiest tenth of all files. */
export function busyFiles(paths: string[], commits: Record<string, { commits: number } | undefined>): { path: string, commits: number }[] {
  const all = Object.values(commits).map(item => item?.commits ?? 0).filter(count => count > 0).sort((a, b) => a - b)
  if (!all.length) return []
  const tenth = all[Math.floor(all.length * 0.9)] ?? all[all.length - 1]!
  const bar = Math.max(8, tenth)
  return paths
    .map(path => ({ path, commits: commits[path]?.commits ?? 0 }))
    .filter(item => item.commits >= bar)
    .sort((a, b) => b.commits - a.commits)
}
