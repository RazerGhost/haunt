import type { Hotspots } from '~~/shared/types'

/*
 * Hotspots: how often each file of the shown repository changed lately (git log --numstat, 180 days), summed per part
 * for the layer map. Only read while the switch is on or a risk line wants them (`want`), and again after every git
 * event. One copy for the app; plugins/haunt.client.ts starts it.
 */

const DAYS = 180
const on = usePersisted('hotspots', false, value => typeof value === 'boolean')
const data = shallowRef<Hotspots | null>(null)
/** How many mounted components want the numbers whatever the switch says (the risk lines). */
const wanted = ref(0)

export function startHotspots(): void {
  const { repo, gitChanged } = repositories
  const headChanged = gitChanged('head')
  const ask = latest(repo)

  async function load(): Promise<void> {
    try {
      const found = await ask.run(at => haunt.hotspots(at, DAYS))
      if (found !== undefined) data.value = found
    } catch {
      data.value = null
    }
  }

  watch([on, headChanged, repo, () => wanted.value > 0], (_now, old) => {
    if (repo.value !== old?.[2]) data.value = null
    if ((on.value || wanted.value > 0) && repo.value) void load()
  }, { immediate: true })
}

export function useHotspots() {
  const { index } = useCode()
  /** Per file: commits that touched it. */
  const files = computed(() => (on.value && data.value ? data.value.files : null))

  /** Per part: file changes summed over its files, and a 0–1 score on a log scale against the busiest part. */
  const parts = computed(() => {
    if (!files.value || !index.value) return null
    const sums = new Map<string, number>()
    for (const [part, list] of index.value.filesByPart) {
      const sum = list.reduce((total, file) => total + (files.value![file.path]?.commits ?? 0), 0)
      if (sum) sums.set(part, sum)
    }
    const top = Math.log1p(Math.max(0, ...sums.values()))
    return new Map([...sums].map(([part, sum]) => [part, { score: top ? Math.log1p(sum) / top : 0, commits: sum }]))
  })

  /** Per file, whatever the switch says, for a component that called `want`. */
  const all = computed(() => data.value?.files ?? null)
  /** Asks for the numbers while the calling component is mounted. */
  function want(): void {
    onMounted(() => wanted.value++)
    onBeforeUnmount(() => wanted.value--)
  }

  return { on, days: DAYS, files, parts, all, want }
}
