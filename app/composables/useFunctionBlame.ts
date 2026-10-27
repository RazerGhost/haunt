import type { FunctionNode } from '~~/shared/types'

/*
 * Who changed each function last, and when: from one `git blame` of its file (as it is in the working tree), the
 * newest commit among the function's lines. Asked for only when switched on (the part view's "Last changed"), for
 * the files shown open, a few at a time; kept until the files or HEAD change.
 */
export interface LastChange {
  author: string
  time: number
  subject: string
  /** Some of its lines are not committed yet. */
  uncommitted: boolean
}

const AT_ONCE = 4
const known = new Map<string, Promise<Map<string, LastChange>>>()
const waiting: (() => void)[] = []
let running = 0

function turn<T>(work: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const go = () => {
      running++
      work().then(resolve, reject).finally(() => {
        running--
        waiting.shift()?.()
      })
    }
    if (running < AT_ONCE) go()
    else waiting.push(go)
  })
}

const NOT_COMMITTED = /^0+$/

function lastChanges(functions: FunctionNode[], lines: { sha: string }[], commits: Map<string, { author: string, time: number, subject: string }>): Map<string, LastChange> {
  const out = new Map<string, LastChange>()
  const walk = (items: FunctionNode[]) => {
    for (const fn of items) {
      let newest: LastChange | null = null
      let uncommitted = false
      for (const line of lines.slice(fn.line - 1, fn.endLine)) {
        if (NOT_COMMITTED.test(line.sha)) {
          uncommitted = true
          continue
        }
        const commit = commits.get(line.sha)
        if (commit && (!newest || commit.time > newest.time)) newest = { ...commit, uncommitted: false }
      }
      if (newest || uncommitted) out.set(fn.id, { author: newest?.author ?? '', time: newest?.time ?? 0, subject: newest?.subject ?? '', uncommitted })
      walk(fn.children)
    }
  }
  walk(functions)
  return out
}

export function useFunctionBlame() {
  const { repo, gitChanged } = useRepositories()
  const { structure: scan } = useCode()
  const on = usePersisted('function-blame', false, value => typeof value === 'boolean')
  // A new scan or a moved HEAD: what was read is out of date.
  watch([() => scan.value?.scannedAt, gitChanged('head'), repo], () => known.clear())

  /** Each function's last change in the file at `path`, by function id; empty when it could not be read. */
  function of(path: string, functions: FunctionNode[]): Promise<Map<string, LastChange>> {
    const at = repo.value
    if (!at) return Promise.resolve(new Map())
    const key = `${at}\0${path}`
    let found = known.get(key)
    if (!found) {
      found = turn(() => haunt.fileBlame(at, path, null))
        .then(blame => lastChanges(functions, blame.lines, new Map(blame.commits.map(commit => [commit.sha, commit]))))
        .catch(() => new Map<string, LastChange>())
      known.set(key, found)
    }
    return found
  }
  return { on, of }
}
