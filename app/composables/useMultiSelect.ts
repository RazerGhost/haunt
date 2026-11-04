/*
 * Picking several items of a list, as a file manager does: a click picks one, Ctrl-click (⌘ on macOS) adds or takes one
 * out, Shift-click picks the run from the item last clicked without Shift, and Ctrl A picks them all when the list is
 * marked data-select-all with `name` (app/plugins/select-all.client.ts). An action on a picked item of several then
 * covers all the picked ones (`covering`). Local changes keeps its own, over its two lists.
 */
export function useMultiSelect<T>(items: () => T[], key: (item: T) => string, name?: string) {
  const picked = ref<Set<string>>(new Set())
  let anchor: string | null = null

  /** Picks `item` as `event` asks (Ctrl, Shift); whether it was a Ctrl- or Shift-click, which stands in for a plain click's own action. */
  function click(item: T, event: MouseEvent): boolean {
    const id = key(item)
    const ctrl = event.ctrlKey || event.metaKey
    if (event.shiftKey) {
      const ids = items().map(key)
      const [a, b] = [ids.indexOf(anchor ?? id), ids.indexOf(id)].sort((x, y) => x - y)
      picked.value = new Set([...ctrl ? picked.value : [], ...ids.slice(Math.max(0, a!), b! + 1)])
      return true
    }
    anchor = id
    if (ctrl) {
      const next = new Set(picked.value)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      picked.value = next
      return true
    }
    picked.value = new Set([id])
    return false
  }

  const isPicked = (item: T) => picked.value.has(key(item))
  /** The picked items in the list's order. */
  const chosen = computed(() => items().filter(item => picked.value.has(key(item))))
  /** Several are picked. */
  const several = computed(() => chosen.value.length > 1)
  /** What an action on `item` covers: every picked item when it is one of several, else `item` alone. */
  const covering = (item: T) => (several.value && isPicked(item) ? chosen.value : [item])
  const clear = () => { picked.value = new Set() }

  // Items that left the list leave the selection.
  watch(() => items().map(key), (ids) => {
    const left = new Set(ids)
    if ([...picked.value].some(id => !left.has(id))) picked.value = new Set([...picked.value].filter(id => left.has(id)))
  })

  if (name) {
    const all = (event: Event) => {
      if ((event as CustomEvent<string>).detail === name) picked.value = new Set(items().map(key))
    }
    onMounted(() => window.addEventListener('haunt:select-all', all))
    onBeforeUnmount(() => window.removeEventListener('haunt:select-all', all))
  }

  return { click, isPicked, chosen, several, covering, clear }
}
