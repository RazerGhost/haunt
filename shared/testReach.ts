/*
 * The tests that reach a change: from each changed function out through its callers, step by step, to the tests whose
 * calls lead to it (the scanner marks a test's callback `test`), and the changed functions no test reaches. A walk
 * stops at a test and goes at most `depth` steps out; a function's callers are those the call graph knows, so a call
 * through an interface or a callback passed around is not followed.
 */

export interface TestReach {
  /** The test's function id. */
  id: string
  /** The changed functions it reaches, each with the fewest steps to it (1: the test calls it). */
  reaches: Map<string, number>
}

export interface Reach {
  /** The tests that reach a changed function, nearest first, then by id. */
  tests: TestReach[]
  /** The changed functions no test reaches, in the order given. */
  unreached: string[]
}

export function testsReaching(changed: Iterable<string>, callersOf: (id: string) => readonly string[], isTest: (id: string) => boolean, depth = 8): Reach {
  const found = new Map<string, Map<string, number>>()
  const unreached: string[] = []
  for (const start of changed) {
    let reached = false
    // A changed test is a test that reaches the change: it is run too.
    if (isTest(start)) {
      reached = true
      note(found, start, start, 0)
    }
    const seen = new Set([start])
    let frontier = [start]
    for (let steps = 1; steps <= depth && frontier.length; steps++) {
      const next: string[] = []
      for (const id of frontier) {
        for (const caller of callersOf(id)) {
          if (seen.has(caller)) continue
          seen.add(caller)
          if (isTest(caller)) {
            reached = true
            note(found, caller, start, steps)
          } else {
            next.push(caller)
          }
        }
      }
      frontier = next
    }
    if (!reached) unreached.push(start)
  }
  const nearest = (test: TestReach) => Math.min(...test.reaches.values())
  const tests = [...found].map(([id, reaches]) => ({ id, reaches }))
  tests.sort((a, b) => nearest(a) - nearest(b) || a.id.localeCompare(b.id))
  return { tests, unreached }
}

function note(found: Map<string, Map<string, number>>, test: string, changed: string, steps: number): void {
  const reaches = found.get(test) ?? found.set(test, new Map()).get(test)!
  reaches.set(changed, Math.min(reaches.get(changed) ?? Infinity, steps))
}
