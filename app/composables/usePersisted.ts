import type { Ref } from 'vue'

/*
 * A setting of the window kept between launches (local storage, under "haunt:"): one ref per key for the whole app,
 * read once, saved on every change. A private window or a full disk keeps it for this visit only.
 */

const kept = new Map<string, Ref<unknown>>()

/**
 * What the window keeps of the repository at `from` (by its path, or `<path>\n…`, in any record or list it keeps) moves
 * to `to`, or is forgotten when `to` is null: its folder moved (Locate), or it was removed.
 */
export function movePersisted(from: string, to: string | null): void {
  if (!import.meta.client || from === to) return
  const moved = (value: unknown): unknown => {
    if (Array.isArray(value)) {
      if (!value.includes(from)) return value
      const kept = value.filter(item => item !== from)
      return to === null || kept.includes(to) ? kept : value.map(item => item === from ? to : item)
    }
    if (!value || typeof value !== 'object') return value
    const record = value as Record<string, unknown>
    const ours = Object.keys(record).filter(key => key === from || key.startsWith(`${from}\n`))
    if (!ours.length) return value
    const next = { ...record }
    for (const key of ours) {
      const kept = next[key]
      delete next[key]
      if (to !== null) next[to + key.slice(from.length)] = kept
    }
    return next
  }
  const keys: string[] = []
  try {
    for (let at = 0; at < localStorage.length; at++) {
      const key = localStorage.key(at)
      if (key?.startsWith('haunt:')) keys.push(key.slice('haunt:'.length))
    }
  } catch { /* nothing kept */ }
  for (const key of new Set([...keys, ...kept.keys()])) {
    const known = kept.get(key)
    if (known) {
      const next = moved(known.value)
      if (next !== known.value) known.value = next
      continue
    }
    try {
      const raw = localStorage.getItem(`haunt:${key}`)
      if (raw === null) continue
      const value = JSON.parse(raw) as unknown
      const next = moved(value)
      if (next !== value) localStorage.setItem(`haunt:${key}`, JSON.stringify(next))
    } catch { /* unreadable: left */ }
  }
}

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
