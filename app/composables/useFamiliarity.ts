import type { Ref } from 'vue'
import type { Located, ScanIndex } from './useCode'
import { familiarKey, familiarity, hashCode, isRecords, type FamiliarRecord, type FamiliarSource, type Familiarity } from '~~/shared/familiarity'

/*
 * The functions you learned, per repository, kept on this machine (local storage, "haunt:familiar"), never in the
 * repository; the rules are in shared/familiarity.ts.
 *
 * How it flows: marking a function (the panel's "Mark as understood", a route's chain recalled, a call path predicted)
 * reads its code now through the `source` command, hashes it and stores the record under its file and qualified name.
 * After every scan, the functions with a record are found again by that name and their code is read and hashed again
 * (a few at a time); `states` compares the two, so a function edited since you learned it shows as stale. Only the
 * functions you learned are read, never the whole repository. One copy for the app; plugins/haunt.client.ts starts it.
 */

type Store = Record<string, Record<string, FamiliarRecord>>
let store: Ref<Store> | null = null
const kept = () => (store ??= usePersisted<Store>('familiar', {}, value => !!value && typeof value === 'object' && !Array.isArray(value) && Object.values(value).every(isRecords)))

/** Each learned function's hash now, by function id: replaced as a whole once a scan's hashes are read. */
const hashes = shallowRef(new Map<string, string>())

/** The names the store knows a function by: its file and the names of the functions it is written in, then its own. */
function keyOf(index: ScanIndex, located: Located): string {
  const names = [located.fn.name]
  let parent = located.parent
  while (parent) {
    const up = index.functions.get(parent)
    if (!up) break
    names.unshift(up.fn.name)
    parent = up.parent
  }
  return familiarKey(located.file.path, names)
}

/* Reading code: a few files at a time, so a repository you learned a lot of does not flood the bridge. */
const AT_ONCE = 4
let running = 0
const waiting: (() => void)[] = []
function turn<T>(work: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const go = () => {
      running++
      work().then(resolve, reject).finally(() => {
        running--
        waiting.shift()?.()
      })
    }
    if (running < AT_ONCE) go()
    else waiting.push(go)
  })
}

/** A function's hash as its file has it now; null when it could not be read. */
function hashNow(repo: string, located: Located): Promise<string | null> {
  const { line, endLine } = located.fn
  return turn(() => haunt.source(repo, located.file.path, line, endLine))
    .then(found => hashCode(found.lines, endLine - line + 1))
    .catch(() => null)
}

/** Starts reading the learned functions' code again after each scan; called by the plugin after the code. */
export function startFamiliarity(): void {
  const { repo } = repositories
  const { index, structure } = useCode()
  const records = computed(() => (repo.value ? kept().value[repo.value] ?? {} : {}))
  let asked = 0
  watch([() => structure.value?.scannedAt, repo, () => Object.keys(records.value).length], async ([, at], old) => {
    if (at !== old?.[1]) hashes.value = new Map()
    const now = index.value
    if (!at || !now) return
    const ask = ++asked
    const byKey = idsByKey(now)
    const found = new Map<string, string>()
    await Promise.all(Object.keys(records.value).map(async (key) => {
      const id = byKey.get(key)
      const located = id ? now.functions.get(id) : undefined
      if (!located) return
      const hash = await hashNow(at, located)
      if (hash) found.set(located.fn.id, hash)
    }))
    // A newer scan or another repository asked since: these hashes are out of date.
    if (ask === asked) hashes.value = found
  }, { immediate: true })
}

/** Every function of the scan by its store name; worked out once per structure. */
let keyed: { index: ScanIndex, byKey: Map<string, string> } | null = null
function idsByKey(index: ScanIndex): Map<string, string> {
  if (keyed?.index.functions === index.functions) return keyed.byKey
  const byKey = new Map<string, string>()
  for (const located of index.functions.values()) byKey.set(keyOf(index, located), located.fn.id)
  keyed = { index, byKey }
  return byKey
}

export function useFamiliarity() {
  const { repo } = useRepositories()
  const { index } = useCode()
  const all = kept()
  const records = computed(() => (repo.value ? all.value[repo.value] ?? {} : {}))

  /** Where each learned function of the scan stands, by function id; a function you never learned is not in it. */
  const states = computed(() => {
    const out = new Map<string, Familiarity>()
    const now = index.value
    if (!now) return out
    const byKey = idsByKey(now)
    for (const [key, record] of Object.entries(records.value)) {
      const id = byKey.get(key)
      const state = id ? familiarity(record, hashes.value.get(id)) : null
      if (id && state) out.set(id, state)
    }
    return out
  })

  const located = (id: string) => index.value?.functions.get(id) ?? null
  /** The record of a function, if you learned it. */
  function recordOf(id: string): FamiliarRecord | null {
    const found = located(id)
    return found && index.value ? records.value[keyOf(index.value, found)] ?? null : null
  }

  /** Marks functions as learned, with their code as it is now. */
  async function mark(ids: Iterable<string>, source: FamiliarSource): Promise<void> {
    const at = repo.value
    const now = index.value
    if (!at || !now) return
    const read = await Promise.all([...new Set(ids)].map(async (id) => {
      const found = now.functions.get(id)
      if (!found) return null
      const hash = await hashNow(at, found)
      return hash ? { id, key: keyOf(now, found), hash } : null
    }))
    if (repo.value !== at) return
    const added: Record<string, FamiliarRecord> = {}
    const next = new Map(hashes.value)
    for (const item of read) {
      if (!item) continue
      added[item.key] = { at: Date.now(), source, hash: item.hash }
      next.set(item.id, item.hash)
    }
    hashes.value = next
    all.value = { ...all.value, [at]: { ...all.value[at], ...added } }
  }

  /** Forgets functions you marked. */
  function unmark(ids: Iterable<string>): void {
    const at = repo.value
    const now = index.value
    if (!at || !now) return
    const rest = { ...all.value[at] }
    for (const id of ids) {
      const found = now.functions.get(id)
      if (found) delete rest[keyOf(now, found)]
    }
    all.value = { ...all.value, [at]: rest }
  }

  return { states, recordOf, mark, unmark }
}
