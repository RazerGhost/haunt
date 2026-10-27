import type { BaseChoice, ChangedFile, ChangedFunction, Changes } from '~~/shared/types'

/*
 * What the working tree changes compared with a base (by default where the branch left main), function by function,
 * and the blast radius: the functions that call a changed one, a few steps out through the call graph. Read again
 * after every scan (a saved file) and on every git event (a commit, a branch switch, staging). One copy for the app;
 * plugins/haunt.client.ts starts it.
 */

/** Null until read; then the changes, or null when the folder is not in a git repository. */
const result = shallowRef<{ changes: Changes | null } | null>(null)
const loading = ref(false)
const problem = ref('')
/** merge-base (default) | uncommitted | ref:<branch, tag or commit>. */
const base = usePersisted('base', 'merge-base', value => typeof value === 'string' && (value === 'merge-base' || value === 'uncommitted' || value.startsWith('ref:')))
/** How many steps out the blast radius goes. */
const depth = ref(2)

function baseChoice(value: string): BaseChoice {
  if (value === 'uncommitted') return { kind: 'uncommitted' }
  if (value.startsWith('ref:')) return { kind: 'ref', ref: value.slice(4) }
  return { kind: 'merge-base', branch: null }
}

export function startChanges(): void {
  const { repo, gitChanged } = repositories
  // Staging and commits, and a fetch that moves the branch the changes are compared with.
  const changesChanged = gitChanged('head', 'index', 'refs')
  const { scan } = useCode()
  const ask = latest(repo)
  let timer: ReturnType<typeof setTimeout> | null = null

  async function load(): Promise<void> {
    loading.value = true
    try {
      const found = await ask.run(at => haunt.changes(at, baseChoice(base.value)))
      if (found === undefined) {
        // No repository any more: nothing is coming after this one.
        if (!repo.value) loading.value = false
        return
      }
      result.value = { changes: found }
      problem.value = ''
      loading.value = false
    } catch (error) {
      problem.value = errorMessage(error, t('composables.useChanges.readFailed'))
      loading.value = false
    }
  }
  function later(): void {
    ask.cancel()
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void load(), 300)
  }

  // A new structure (a save, another tab) or news from .git: read the changes again.
  watch([repo, () => scan.value?.scannedAt], ([now], old) => {
    // Another tab: the old repository's changes go at once, rather than showing until the new ones are in.
    if (now !== old?.[0]) {
      result.value = null
      problem.value = ''
    }
    // Only once the first structure is in: the scanner answers after it, and a call left waiting holds a connection.
    if (now && scan.value) later()
  }, { immediate: true })
  watch(changesChanged, () => scan.value && later())
  watch(base, () => scan.value && void load())
}

export function useChanges() {
  const { index } = useCode()
  const changes = computed(() => result.value?.changes ?? null)
  const codeFiles = computed<ChangedFile[]>(() => changes.value?.files.filter(file => file.code) ?? [])
  const otherFiles = computed<ChangedFile[]>(() => changes.value?.files.filter(file => !file.code) ?? [])
  const functions = computed<(ChangedFunction & { path: string })[]>(() => codeFiles.value.flatMap(file => file.functions.map(fn => ({ ...fn, path: file.path }))))
  /** Changed functions by their id in the scan (removed ones have none). */
  const changedById = computed(() => new Map(functions.value.filter(fn => fn.id).map(fn => [fn.id!, fn])))

  /** The functions that reach a changed one through calls, up to `depth` steps, nearest first (utils/impact.ts). */
  const affected = computed(() => blastRadius(index.value, new Set(changedById.value.keys()), depth.value))
  /** The affected functions that have a name: anonymous callbacks carry the walk on but are left out of lists and counts. */
  const affectedNamed = computed(() => namedOnly(index.value, affected.value))
  /** Per part: changed functions and affected ones, for the layer map's badges. */
  const byPart = computed(() => countsByPart(index.value, codeFiles.value, affectedNamed.value))

  const summary = computed(() => ({
    files: changes.value?.files.length ?? 0,
    functions: functions.value.length,
    added: codeFiles.value.reduce((sum, file) => sum + file.added, 0),
    removed: codeFiles.value.reduce((sum, file) => sum + file.removed, 0),
    affected: affectedNamed.value.size
  }))

  return { result, changes, codeFiles, otherFiles, functions, changedById, affected, affectedNamed, byPart, summary, base, depth, loading, problem }
}
