/*
 * The code that is there for the tests: the functions in test files, and the helpers only tests call (a `checkPage` in
 * `test-helpers.ts`, wherever it lives), and the helpers only those helpers call. Left out of the Calls page's counts
 * unless you ask for tests, so its "Most called" shows the code the app runs, not what a hundred tests call.
 *
 * A function joins once it has callers and every one of them is test code; one the app calls too stays out. Worked
 * from the test files outwards, so a set of helpers that only call each other and nothing else reaches is left alone.
 */

export function testSupport(
  ids: Iterable<string>,
  inTestFile: (id: string) => boolean,
  callersOf: (id: string) => readonly string[],
  calleesOf: (id: string) => readonly string[]
): Set<string> {
  const support = new Set<string>()
  const waiting: string[] = []
  for (const id of ids) {
    if (inTestFile(id)) {
      support.add(id)
      waiting.push(id)
    }
  }
  // Each function added may make the functions it calls test-only too.
  while (waiting.length) {
    for (const next of calleesOf(waiting.pop()!)) {
      if (support.has(next)) continue
      const callers = callersOf(next)
      if (callers.length && callers.every(caller => support.has(caller))) {
        support.add(next)
        waiting.push(next)
      }
    }
  }
  return support
}
