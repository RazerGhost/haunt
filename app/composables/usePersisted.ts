import type { Ref } from 'vue'

/*
 * A setting of the window kept between launches (local storage, under "haunt:"): one ref per key for the whole app,
 * read once, saved on every change. A private window or a full disk keeps it for this visit only.
 */

const kept = new Map<string, Ref<unknown>>()

export function usePersisted<T>(key: string, fallback: T, valid: (value: unknown) => boolean = () => true): Ref<T> {
  const known = kept.get(key)
  if (known) return known as Ref<T>
  let start = fallback
  try {
    const raw = import.meta.client ? localStorage.getItem(`haunt:${key}`) : null
    if (raw !== null) {
      const read = JSON.parse(raw) as unknown
      if (valid(read)) start = read as T
    }
  } catch { /* unreadable: the fallback */ }
  const value = ref(start) as Ref<T>
  watch(value, (next) => {
    try {
      localStorage.setItem(`haunt:${key}`, JSON.stringify(next))
    } catch { /* the setting lasts this visit */ }
  }, { deep: true })
  kept.set(key, value as Ref<unknown>)
  return value
}
