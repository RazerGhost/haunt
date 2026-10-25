import type { CallEdge, FunctionNode, Part, Scan, ScannedFile } from '~~/shared/types'
import type { Calls } from '~/utils/bridge'
import { addTo } from '~~/shared/collect'

/*
 * The code of the shown repository, as the scanner read it: the structure (parts, files, functions) and, a little
 * later, the call graph, with the lookups every view needs. The two arrive separately, so a save sends the structure
 * once and the calls once, never the whole scan twice. Replaced as a whole and never changed in place, so they are held
 * in shallow refs: thousands of functions need no deep reactivity. One copy for the app; plugins/haunt.client.ts
 * starts it.
 */

export interface Located {
  fn: FunctionNode
  file: ScannedFile
  /** The function it is written in, if any. */
  parent: string | null
}

export interface ScanIndex {
  functions: Map<string, Located>
  callees: Map<string, CallEdge[]>
  callers: Map<string, CallEdge[]>
  parts: Map<string, Part>
  filesByPart: Map<string, ScannedFile[]>
}

type Status = 'loading' | 'scanning' | 'ready' | 'error' | 'no-root'

const structure = shallowRef<Scan | null>(null)
const calls = shallowRef<Calls | null>(null)
const status = ref<Status>('loading')
const message = ref('')
const search = ref('')

/** The scan with its call graph. After a save the last call graph stays until the new one is in, rather than an
 * empty view; null before the first one. */
const scan = computed<Scan | null>(() => (structure.value ? { ...structure.value, calls: calls.value?.calls ?? null } : null))

/** The structure alone, for the views that need no call graph (read-only to them). */
const structureShown = computed(() => structure.value)

/** Every function by id, the parts, and the files of each part: rebuilt only when the structure changes. */
const byStructure = computed(() => {
  const current = structure.value
  if (!current) return null
  const functions = new Map<string, Located>()
  const filesByPart = new Map<string, ScannedFile[]>()
  for (const file of current.files) {
    const walk = (items: FunctionNode[], parent: string | null) => items.forEach((fn) => {
      functions.set(fn.id, { fn, file, parent })
      walk(fn.children, fn.id)
    })
    walk(file.functions, null)
    addTo(filesByPart, file.part, file)
  }
  return { functions, filesByPart, parts: new Map(current.parts.map(part => [part.name, part])) }
})

/** Callers and callees of each function: rebuilt only when the calls change. */
const byCalls = computed(() => {
  const callees = new Map<string, CallEdge[]>()
  const callers = new Map<string, CallEdge[]>()
  const add = (map: Map<string, CallEdge[]>, key: string, edge: CallEdge) => {
    const list = map.get(key)
    if (list) list.push(edge)
    else map.set(key, [edge])
  }
  for (const edge of calls.value?.calls ?? []) {
    add(callees, edge.from, edge)
    add(callers, edge.to, edge)
  }
  return { callees, callers }
})

const index = computed<ScanIndex | null>(() => (byStructure.value ? { ...byStructure.value, ...byCalls.value } : null))

export function countFunctions(items: FunctionNode[]): number {
  return items.reduce((sum, fn) => sum + 1 + countFunctions(fn.children), 0)
}

export function matchesSearch(items: FunctionNode[], term: string): number {
  return items.reduce((sum, fn) => sum + (fn.name.toLowerCase().includes(term) ? 1 : 0) + matchesSearch(fn.children, term), 0)
}

/** Reads the structure and the calls of the shown repository; called by the plugin after the repositories. */
export function startCode(): void {
  const { repo, info, onNews } = repositories
  const askStructure = latest(repo)
  const askCalls = latest(repo)

  async function loadStructure(): Promise<void> {
    try {
      const found = await askStructure.run(at => haunt.structure(at))
      // Null: the first scan is not in yet; its 'scan' news asks again.
      if (!found) return
      structure.value = found
      if (status.value === 'loading' || status.value === 'error') {
        status.value = 'scanning'
        message.value = 'Working out the call graph'
      }
      void loadCalls()
    } catch (error) {
      status.value = 'error'
      message.value = errorMessage(error, 'The repository could not be scanned.')
    }
  }

  async function loadCalls(): Promise<void> {
    try {
      const found = await askCalls.run(at => haunt.calls(at))
      if (found === undefined || found === null) return
      calls.value = found
      if (found.scannedAt === structure.value?.scannedAt) {
        status.value = 'ready'
        message.value = ''
      }
    } catch { /* the next news reads them again */ }
  }

  // Another repository shown: the old one's code goes at once; the new one's last scan comes back quickly if Rust kept it.
  watch(repo, (now) => {
    askStructure.cancel()
    askCalls.cancel()
    structure.value = null
    calls.value = null
    search.value = ''
    if (now) {
      status.value = 'loading'
      message.value = 'Scanning'
      void loadStructure()
    } else {
      status.value = info.value ? 'no-root' : 'loading'
      message.value = ''
    }
  }, { immediate: true })
  watch(info, (now) => {
    if (now && !now.root) status.value = 'no-root'
  })

  onNews((news) => {
    if (news.type === 'scanning') {
      status.value = 'scanning'
      message.value = news.reason === 'start' ? 'Scanning' : `Rescanning after ${news.reason}`
    } else if (news.type === 'scan') {
      if (!news.complete) {
        if (news.scannedAt !== structure.value?.scannedAt) void loadStructure()
        status.value = 'scanning'
        message.value = 'Working out the call graph'
      } else {
        if (news.scannedAt !== structure.value?.scannedAt) void loadStructure()
        else void loadCalls()
      }
    } else if (news.type === 'error') {
      status.value = 'error'
      message.value = news.message
    }
  })
}

export function useCode() {
  const { repo } = repositories
  const { includeTests, editor } = usePreferences()
  const route = useRoute()
  const router = useRouter()
  const toast = useToast()

  const selected = computed(() => {
    const id = typeof route.query.fn === 'string' ? route.query.fn : null
    return id ? index.value?.functions.get(id) ?? null : null
  })

  function select(id: string | null, path?: string): void {
    void router.push({ path: path ?? route.path, query: { ...route.query, fn: id ?? undefined } })
  }

  function visible(file: ScannedFile): boolean {
    return includeTests.value || !file.test
  }

  async function open(path: string, line = 1): Promise<void> {
    if (!repo.value) return
    try {
      await haunt.openFile(repo.value, editor.value, path, line)
    } catch (error) {
      toast.add({ title: 'Could not open the editor', description: errorMessage(error, 'Try another editor.'), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }

  async function rescan(): Promise<void> {
    if (repo.value) await haunt.rescan(repo.value)
  }

  /** How each TypeScript-family function is used (the scanner's call pass); null until it is worked out. */
  const uses = computed(() => calls.value?.uses ?? null)

  // `structure` is the scan without its call graph: what only needs the files, parts and layers follows it, and is
  // not worked out again when the call graph arrives.
  return { scan, structure: structureShown, index, uses, status, message, search, selected, select, visible, open, rescan }
}
