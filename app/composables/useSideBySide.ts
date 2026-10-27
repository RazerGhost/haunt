import type { Ref } from 'vue'

/*
 * Diffs side by side or unified: one choice for every diff (kept between launches), shown side by side only when the
 * diff has the room. Below SIDE_BY_SIDE pixels each side would be too narrow to read, so it shows unified until it is
 * wider again; the choice stays as it was.
 */
const SIDE_BY_SIDE = 760

export function useSideBySide(root: Ref<HTMLElement | null | undefined>) {
  const wanted = usePersisted('diff-split', false, value => typeof value === 'boolean')
  const width = ref(Number.POSITIVE_INFINITY)
  let watcher: ResizeObserver | null = null
  onMounted(() => {
    if (!root.value) return
    watcher = new ResizeObserver(([entry]) => { if (entry) width.value = entry.contentRect.width })
    watcher.observe(root.value)
  })
  onBeforeUnmount(() => watcher?.disconnect())
  /** Too narrow for two sides now. */
  const narrow = computed(() => width.value < SIDE_BY_SIDE)
  /** What is shown: side by side when chosen and there is room. */
  const split = computed(() => wanted.value && !narrow.value)
  return { wanted, narrow, split }
}
