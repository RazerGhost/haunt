/*
 * The risk of a change in one line, from what Haunt measures already: how many functions call a changed one (two steps
 * out), how many changed functions no test reaches, how many busy files it touches (changed often lately), and how big
 * it is. Each counts for points; the points give low, medium or high, and the reasons say why (as kinds with their
 * counts and names: the window puts them into words in its language). A guide for where to look first, not a verdict.
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
  reasons: RiskReason[]
}

/** Why a change is as risky as it is, one reason each. */
export type RiskReason =
  /** `count` functions call what changed. */
  | { kind: 'callers', count: number }
  /** `count` of the `of` changed functions whose tests Haunt reads no test reaches. */
  | { kind: 'unreached', count: number, of: number }
  /** Tests reach all `count` changed functions; `partly` when some are in a language whose tests Haunt does not read. */
  | { kind: 'reached', count: number, partly: boolean }
  /** Tests not read for `count` changed functions (PHP, Rust or Lua). */
  | { kind: 'unread', count: number }
  /** Touches `count` busy files, the busiest `name` with `commits` commits in `days` days. */
  | { kind: 'busy', count: number, name: string, commits: number, days: number }
  /** `count` functions changed. */
  | { kind: 'size', count: number }

export function riskOf(input: RiskInput): Risk {
  let points = 0
  const reasons: RiskReason[] = []
  if (input.callers >= 20) points += 2
  else if (input.callers >= 5) points += 1
  if (input.callers) reasons.push({ kind: 'callers', count: input.callers })

  const read = input.changed - (input.unread ?? 0)
  if (input.unreached) {
    points += input.unreached * 2 >= read ? 2 : 1
    reasons.push({ kind: 'unreached', count: input.unreached, of: read })
  } else if (read) {
    reasons.push({ kind: 'reached', count: read, partly: !!input.unread })
  }
  if (input.unread) reasons.push({ kind: 'unread', count: input.unread })

  if (input.busy.length) {
    points += input.busy.length >= 3 ? 2 : 1
    const first = input.busy[0]!
    const name = first.path.split('/').pop() ?? first.path
    reasons.push({ kind: 'busy', count: input.busy.length, name, commits: first.commits, days: input.days })
  }

  if (input.changed >= 15) {
    points += 1
    reasons.push({ kind: 'size', count: input.changed })
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
