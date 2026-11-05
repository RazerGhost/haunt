import type { ComputedRef } from 'vue'
import { testSupport } from '~~/shared/testSupport'

/*
 * The functions that are there for the tests (shared/testSupport.ts): in test files, or called only by tests and their
 * helpers. Worked out once per call graph for the whole app. The Calls page leaves them out of its counts unless its
 * "Include tests" switch is on (kept between launches); the function panel lists callers from tests apart.
 */
let support: ComputedRef<Set<string>> | null = null

export function useTestSupport() {
  const { index } = useCode()
  support ??= computed(() => {
    const at = index.value
    if (!at) return new Set<string>()
    const callers = (id: string) => (at.callers.get(id) ?? []).map(edge => edge.from)
    const callees = (id: string) => (at.callees.get(id) ?? []).map(edge => edge.to)
    return testSupport(at.functions.keys(), id => !!at.functions.get(id)?.file.test, callers, callees)
  })
  const found = support
  const includeTests = usePersisted('calls-tests', false, value => typeof value === 'boolean')
  /** Whether a function is test code: in a test file, or only reached from tests. */
  const isTestCode = (id: string) => found.value.has(id)
  return { includeTests, isTestCode }
}
