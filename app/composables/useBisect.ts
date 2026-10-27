import type { BisectMark, BisectState, CommitInfo } from '~/bindings'

/*
 * Bisecting (src-tauri/src/git/write/bisect.rs): finding the first bad commit between a good one and a bad one, step by
 * step. Where it stands is read when a repository is shown and on every git news; a banner shows it on the Git screens
 * and the Bisect screen starts and steers it. Started from a function, the commits that touched it (its history, read
 * at the start and kept per repository) are suggested first among the ones still in play.
 * One copy for the app; plugins/haunt.client.ts starts it.
 */

const state = shallowRef<BisectState | null>(null)
/** Per repository: the function a bisect was started from, and the commits that touched it. */
const focus = usePersisted<Record<string, { name: string, commits: CommitInfo[] }>>('bisect-focus', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))

export function startBisect(): void {
  const { repo, gitChanged } = repositories
  const changed = gitChanged('head', 'refs')
  const ask = latest(repo)
  async function load(): Promise<void> {
    try {
      const found = await ask.run(at => haunt.bisectState(at))
      if (found !== undefined) state.value = found
    } catch { /* the next news reads it again */ }
  }
  watch(repo, (now) => {
    state.value = null
    if (now) void load()
  }, { immediate: true })
  watch(changed, () => repo.value && void load())
}

export function useBisect() {
  const { repo } = useRepositories()
  const failure = useFailure()
  const busy = ref(false)

  async function run(what: string, write: (at: string) => Promise<unknown>): Promise<boolean> {
    const at = repo.value
    if (!at || busy.value) return false
    busy.value = true
    try {
      await write(at)
      state.value = await haunt.bisectState(at)
      return true
    } catch (error) {
      failure.failed(what, error, t('composables.useBisect.fallback'))
      return false
    } finally {
      busy.value = false
    }
  }

  /** Starts between `bad` and `good`; from a function (`path`, its lines and name), its commits are suggested first. */
  async function start(bad: string, good: string, from?: { path: string, start: number, end: number, name: string }): Promise<boolean> {
    const at = repo.value
    if (!at) return false
    if (from) {
      const history = await haunt.history(at, from.path, from.start, from.end).catch(() => null)
      focus.value = { ...focus.value, [at]: { name: from.name, commits: history?.commits ?? [] } }
    } else {
      const { [at]: _, ...rest } = focus.value
      focus.value = rest
    }
    return run(t('composables.useBisect.notStarted'), where => haunt.bisectStart(where, bad, good))
  }
  const mark = (how: BisectMark) => run(t('composables.useBisect.notMarked'), where => haunt.bisectMark(where, how))
  const jump = (sha: string) => run(t('composables.useBisect.notCheckedOut'), where => haunt.bisectJump(where, sha))
  const stop = () => run(t('composables.useBisect.notStopped'), where => haunt.bisectStop(where))

  /** The commits that touched the function it was started from, among the ones still in play. */
  const suggested = computed(() => {
    const kept = repo.value ? focus.value[repo.value] : undefined
    const inPlay = new Set(state.value?.candidates ?? [])
    return kept ? { name: kept.name, commits: kept.commits.filter(commit => inPlay.has(commit.sha)) } : null
  })

  return { state, busy, start, mark, jump, stop, suggested }
}
