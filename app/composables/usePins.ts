import type { Located } from './useCode'

/*
 * Pinned functions: the ones you come back to, kept per repository between launches, a click away in the Code sidebar
 * and the palette. A pin is a function's file and name (its id holds its line, which moves as the code is edited), and
 * finds the function again in each scan; one that is gone shows as gone until unpinned.
 */
interface Pin { path: string, name: string }

export function usePins() {
  const { repo } = useRepositories()
  const { index } = useCode()
  const all = usePersisted<Record<string, Pin[]>>('pins', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
  const pins = computed(() => (repo.value ? all.value[repo.value] ?? [] : []))

  const find = (pin: Pin): Located | null => {
    for (const located of index.value?.functions.values() ?? []) {
      if (located.file.path === pin.path && located.fn.name === pin.name) return located
    }
    return null
  }
  /** The pinned functions as the scan has them now; `found` is null for one that is gone. */
  const pinned = computed(() => pins.value.map(pin => ({ ...pin, found: find(pin) })))

  function isPinned(located: Located | null): boolean {
    return !!located && pins.value.some(pin => pin.path === located.file.path && pin.name === located.fn.name)
  }
  function toggle(located: Located): void {
    if (!repo.value) return
    const pin = { path: located.file.path, name: located.fn.name }
    const rest = pins.value.filter(other => other.path !== pin.path || other.name !== pin.name)
    all.value = { ...all.value, [repo.value]: rest.length === pins.value.length ? [...rest, pin] : rest }
  }
  function unpin(pin: Pin): void {
    if (repo.value) all.value = { ...all.value, [repo.value]: pins.value.filter(other => other.path !== pin.path || other.name !== pin.name) }
  }
  return { pinned, isPinned, toggle, unpin }
}
