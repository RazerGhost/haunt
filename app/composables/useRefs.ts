import type { Refs } from '~~/shared/types'

/*
 * The shown repository's branches, remote branches, tags and stashes for the sidebar and the toolbar, and its count of
 * local changes. Read as soon as a repository is shown (they do not wait for its scan). The refs are read again when
 * HEAD, a ref or the stash changes; the count, which needs `git status` and is the slow one, when HEAD or the index
 * changes and after every scan (a save). One copy for the app; plugins/haunt.client.ts starts it.
 */

/** Null until read; then the refs, or null when the folder is not in a git repository. */
const result = shallowRef<{ refs: Refs | null } | null>(null)
const count = ref<number | null>(null)

export function startRefs(): void {
  const { repo, gitChanged } = repositories
  const { scan } = useCode()
  const refsChanged = gitChanged('head', 'refs', 'stash')
  const countChanged = gitChanged('head', 'index')
  const askRefs = latest(repo)
  const askCount = latest(repo)
  let timer: ReturnType<typeof setTimeout> | null = null

  // A failed read is tried again soon while nothing was read yet (Rust may still be starting); after that, the next
  // event reads it again.
  const retry = (load: () => Promise<void>, at: string | null) => setTimeout(() => repo.value === at && void load(), 2000)
  async function loadRefs(): Promise<void> {
    const at = repo.value
    try {
      const found = await askRefs.run(at => haunt.refs(at))
      if (found !== undefined) result.value = { refs: found }
    } catch {
      if (!result.value) retry(loadRefs, at)
    }
  }
  async function loadCount(): Promise<void> {
    const at = repo.value
    try {
      const found = await askCount.run(at => haunt.localChanges(at))
      if (found !== undefined) count.value = found
    } catch {
      if (count.value === null) retry(loadCount, at)
    }
  }

  watch(repo, (now) => {
    result.value = null
    count.value = null
    askRefs.cancel()
    askCount.cancel()
    if (timer) clearTimeout(timer)
    if (now) timer = setTimeout(() => void Promise.all([loadRefs(), loadCount()]), 100)
  }, { immediate: true })
  watch(refsChanged, () => repo.value && void loadRefs())
  watch([countChanged, () => scan.value?.scannedAt], () => repo.value && void loadCount())
}

export function useRefs() {
  const refs = computed(() => result.value?.refs ?? null)
  const current = computed(() => refs.value?.branches.find(branch => branch.current) ?? null)
  /** Files with changes not yet committed; null until counted or outside a repository. */
  const localChanges = computed(() => count.value)
  /** Git answered that this folder is not a repository (not just: the refs are still on their way). */
  const notRepository = computed(() => result.value !== null && result.value.refs === null)
  return { refs, current, localChanges, notRepository }
}
