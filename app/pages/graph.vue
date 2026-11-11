<script lang="ts">
/** The focuses shown, in order, for the trail (going back cuts it); kept while Haunt runs. */
const trail = ref<string[]>([])
/** Every function put in the middle, the latest first, for the Recent list. */
const recent = ref<string[]>([])
</script>

<script setup lang="ts">
import { focusGraph, pathGraph, projectGraph, type CodeGraph as Graph, type Focus, type GraphCard, type GraphEdge } from '~~/shared/codeGraph'
import { entryPoints, pathsTo, pathView } from '~~/shared/callPaths'
import { candidates, grade, puzzle, type Puzzle } from '~~/shared/predict'
import type { FunctionNode } from '~~/shared/types'
import { graphSvg, type SvgGraphCard, type SvgGraphLine } from '~~/shared/exportSvg'
import { layoutCards, type Placed } from '~/utils/cardLayout'
import type { SlotLook } from '~/components/CodeGraph.vue'

/*
 * The code graph: files as cards with their functions as rows, and the calls between them (shared/codeGraph.ts). It
 * starts from a focus, never the whole repository:
 * - a function (`?center=`): the files of what calls it and what it calls, Around; or how the code reaches it from
 *   its entry points, Paths (shared/callPaths.ts), where Predict hides the functions between to fill in yourself;
 * - a file (`?file=`): all its functions, and the files they call and are called from;
 * - a part (`?part=`): its files, the rows on a call, the files outside it shut;
 * - nothing: the project, its parts as cards and the files that import across them as rows, judged by the project's
 *   rules (red breaks one, amber breaks one the project accepts for now).
 *
 * Click a function to see it in the panel, double-click to put it in the middle; click a card's name to focus on its
 * file or part. Start from (most called, entry points, recent; typing in the search box opens it) and the
 * trail above move between focuses; Show ticks what is drawn besides the calls.
 */
const { scan, index, selected, search, select, visible, open } = useCode()
const bookmarks = useBookmarks()
const route = useRoute()
const router = useRouter()
const { changedById } = useChanges()
const { includeTests, isTestCode } = useTestSupport()
const { on: hotspots, files: churn, days } = useHotspots()
const familiarity = usePersisted('familiarity-overlay', false, value => typeof value === 'boolean')
const { states, mark } = useFamiliarity()

const query = (key: string) => (typeof route.query[key] === 'string' ? route.query[key] as string : null)
const focus = computed<Focus>(() => {
  const center = query('center')
  if (center) return { kind: 'function', id: center }
  const file = query('file')
  if (file) return { kind: 'file', path: file }
  const part = query('part')
  if (part) return { kind: 'part', name: part }
  return { kind: 'project' }
})
const focusKey = (at: Focus) => (at.kind === 'function' ? `fn:${at.id}` : at.kind === 'file' ? `file:${at.path}` : at.kind === 'part' ? `part:${at.name}` : 'all')
const center = computed(() => (focus.value.kind === 'function' && index.value?.functions.has(focus.value.id) ? focus.value.id : null))

// From the function panel, the sidebar or the palette only the function is given: it goes in the middle.
watch(() => [route.query.center, route.query.file, route.query.part, route.query.fn], () => {
  const fn = query('fn')
  if (!query('center') && !query('file') && !query('part') && fn) void router.replace({ query: { ...route.query, center: fn } })
}, { immediate: true })

function go(next: Focus, keepPanel = true): void {
  const fn = keepPanel && !predicting.value ? route.query.fn : undefined
  if (next.kind === 'function') void router.push({ query: { center: next.id, fn: predicting.value ? undefined : next.id } })
  else if (next.kind === 'file') void router.push({ query: { file: next.path, fn } })
  else if (next.kind === 'part') void router.push({ query: { part: next.name, fn } })
  else void router.push({ query: {} })
}
const recenter = (id: string) => go({ kind: 'function', id })
function goKey(key: string): void {
  if (key === 'all') go({ kind: 'project' }, false)
  else if (key.startsWith('fn:')) recenter(key.slice(3))
  else if (key.startsWith('file:')) go({ kind: 'file', path: key.slice(5) })
  else go({ kind: 'part', name: key.slice(5) })
}

/** Cards the viewer opened or shut, for this focus only. */
const opened = ref(new Set<string>())
const flipped = ref(new Set<string>())
watch(() => focusKey(focus.value), (key) => {
  opened.value = new Set()
  flipped.value = new Set()
  if (key === 'all') return
  const at = trail.value.indexOf(key)
  trail.value = at >= 0 ? trail.value.slice(0, at + 1) : [...trail.value, key].slice(-30)
  if (key.startsWith('fn:')) recent.value = [key.slice(3), ...recent.value.filter(other => other !== key.slice(3))].slice(0, 30)
}, { immediate: true })
const crumbs = computed(() => trail.value.slice(-5))
function back(): void {
  const at = trail.value.indexOf(focusKey(focus.value))
  if (at > 0) goKey(trail.value[at - 1]!)
  else go({ kind: 'project' }, false)
}
const baseOf = (path: string) => path.split('/').pop() ?? path
const name = (id: string) => index.value?.functions.get(id)?.fn.name ?? id
function crumbLabel(key: string): string {
  if (key.startsWith('fn:')) return name(key.slice(3))
  if (key.startsWith('file:')) return baseOf(key.slice(5))
  return key.slice(5)
}
const crumbIcon = (key: string) => (key.startsWith('fn:') ? 'i-lucide-square-function' : key.startsWith('file:') ? 'i-lucide-file' : 'i-lucide-box')

/* Start from: the lists to begin at */
type Tab = 'called' | 'entries' | 'recent'
const tab = ref<Tab>('called')
const tabs = ['called', 'entries', 'recent'] as const
const term = computed(() => search.value.trim().toLowerCase())
/** Test code (test files, and helpers only tests call) is left out of the lists, counts and graphs unless asked for. */
const included = (id: string) => includeTests.value || !isTestCode(id)
const shown = (id: string) => {
  const found = index.value?.functions.get(id)
  return !!found && !found.fn.anonymous && included(id)
}
const callersOf = (id: string) => (index.value?.callers.get(id) ?? []).map(edge => edge.from)
const calleesOf = (id: string) => (index.value?.callees.get(id) ?? []).map(edge => edge.to)
const callerCounts = computed(() => {
  const counts = new Map<string, number>()
  for (const [id, edges] of index.value?.callers ?? []) counts.set(id, edges.filter(edge => included(edge.from)).length)
  return counts
})
const callerCount = (id: string) => callerCounts.value.get(id) ?? 0
const mostCalled = computed(() => [...index.value?.functions.keys() ?? []].filter(shown).sort((a, b) => callerCount(b) - callerCount(a)))
const choices = computed(() => {
  const at = index.value
  if (!at) return []
  let ids: string[]
  if (term.value) ids = mostCalled.value.filter(id => name(id).toLowerCase().includes(term.value))
  else if (tab.value === 'entries') ids = entryPoints(mostCalled.value, id => callersOf(id).filter(shown), id => calleesOf(id).filter(shown))
  else if (tab.value === 'recent') ids = recent.value.filter(id => at.functions.has(id))
  else ids = mostCalled.value
  return ids.slice(0, 60).map(id => ({ id, found: at.functions.get(id)!, callers: callerCount(id), callees: calleesOf(id).filter(included).length }))
})
const fileChoices = computed(() => (term.value ? (scan.value?.files ?? []).filter(file => visible(file) && file.path.toLowerCase().includes(term.value)).slice(0, 20) : []))

/* The options */
const mode = usePersisted<'around' | 'paths'>('graph-mode', 'around', value => value === 'paths' || value === 'around')
const depth = ref(1)
const below = ref(1)
const direction = ref<'both' | 'callers' | 'callees'>('both')
const directions = computed(() => (['both', 'callers', 'callees'] as const).map(value => ({ value, label: t(`pages.graph.direction.${value}`) })))
const withImports = usePersisted('graph-imports', false, value => typeof value === 'boolean')
const typeOnly = ref(true)
const onlyBreaks = ref(false)
const overTime = ref(false)
const rules = computed(() => scan.value?.rules ?? null)
const importOptions = computed(() => (scan.value ? { between: scan.value.imports, inner: scan.value.innerImports, typeOnly: typeOnly.value, rules: !!rules.value, onlyBreaks: onlyBreaks.value } : null))
const pathsMode = computed(() => focus.value.kind === 'function' && mode.value === 'paths')

/* The graph */
const options = computed(() => ({
  depth: depth.value,
  direction: direction.value,
  keep: shown,
  keepFile: (file: { test: boolean }) => includeTests.value || !file.test,
  opened: opened.value,
  flipped: flipped.value,
  imports: withImports.value ? importOptions.value : null
}))
const paths = computed(() => (pathsMode.value && center.value && scan.value?.calls ? pathView(center.value, callersOf, calleesOf, { down: below.value, keep: shown }) : null))

const graph = computed<Graph | null>(() => {
  const at = index.value
  if (!at || !scan.value) return null
  if (focus.value.kind === 'project') return importOptions.value ? projectGraph(scan.value.parts, at.filesByPart, importOptions.value, { keepFile: options.value.keepFile, flipped: flipped.value, layerOrder: scan.value.layers.map(layer => layer.id) }) : null
  if (!scan.value.calls) return null
  if (focus.value.kind === 'function') {
    if (!center.value) return null
    if (predicting.value) return predictGraph.value
    if (paths.value) {
      // Each function in the column of its step: the entry points left, the function in the middle, what it calls right.
      const steps = new Map(paths.value.rows.flatMap(row => row.ids.map(id => [id, row.step] as const)))
      return pathGraph(at, paths.value.rows.flatMap(row => row.ids), paths.value.edges, center.value, options.value, steps)
    }
  }
  return focusGraph(at, focus.value, options.value)
})

/** Where the cards go, worked out after the graph changes; the latest one wins. */
const places = shallowRef(new Map<string, Placed>())
const placing = ref(false)
let asked = 0
watch(graph, async (now) => {
  const ask = ++asked
  if (!now) {
    places.value = new Map()
    return
  }
  placing.value = true
  try {
    const found = await layoutCards(now)
    if (ask === asked) places.value = found
  } finally {
    if (ask === asked) placing.value = false
  }
}, { immediate: true })

/** Per file, a 0–1 score of how often it changed, on a log scale against the busiest file. */
const heat = computed(() => {
  if (!churn.value) return null
  const counts = Object.entries(churn.value).map(([path, item]) => [path, item?.commits ?? 0] as const)
  const top = Math.log1p(Math.max(0, ...counts.map(([, commits]) => commits)))
  return new Map(counts.map(([path, commits]) => [path, top ? Math.log1p(commits) / top : 0]))
})

/* What is shown, in words. */
const summary = computed(() => {
  const now = graph.value
  if (!now) return ''
  if (focus.value.kind === 'function' && paths.value && center.value && !predicting.value) {
    const found = paths.value
    const who = name(center.value)
    const below = calleesOf(center.value).filter(shown).length
    const calls = below ? t('pages.graph.itCalls', { count: below }) : t('pages.graph.itCallsNothing')
    if (found.isEntry) return `${t('pages.graph.isEntry', { name: who })} ${calls}`
    if (found.cyclic) return `${t('pages.graph.inCycle', { name: who })} ${calls}`
    const steps = found.shortest === found.longest ? t('pages.graph.steps', { count: found.shortest }) : t('pages.graph.stepsRange', { shortest: found.shortest, longest: found.longest })
    return `${t(found.paths >= 12 ? 'pages.graph.reachedShortest' : 'pages.graph.reached', { name: who, count: found.entries.length, steps })} ${calls}`
  }
  if (focus.value.kind === 'project') return ''
  const files = now.cards.filter(card => card.kind === 'file').length
  const fns = now.cards.reduce((sum, card) => sum + card.rows.filter(row => !row.context && row.kind === 'function').length, 0)
  const base = t('pages.graph.shown', { files: t('common.files', { count: files }), functions: t('common.functions', { count: fns }) })
  return now.leftOut ? `${base} ${t('pages.graph.leftOut', { count: now.leftOut })}` : base
})
const importSummary = computed(() => {
  const all = (scan.value?.imports ?? []).flatMap(group => group.files).filter(file => typeOnly.value || !file.typeOnly)
  const breaks = all.filter(file => file.breaks.length)
  return { imports: all.length, known: breaks.filter(file => file.known).length, new: breaks.filter(file => !file.known).length }
})
/** The rule breaks there are now, type-only ones too, for the history worked back from it. */
const allBreaks = computed(() => (scan.value?.imports ?? []).flatMap(group => group.files).filter(file => file.breaks.length).length)

/*
 * Predict (Paths only): the entry points and the function stay, the functions between them are numbered places to
 * fill in (shared/predict.ts). Click a place, then a function from the list under the toolbar (the real ones among
 * decoys from the same parts) or from Start from, which the search box narrows to any function. Reveal shows
 * the real paths with each pick right or wrong; the right ones are marked familiar (useFamiliarity, `call-predict`).
 */
const predict = usePersisted('graph-predict', false, value => typeof value === 'boolean')
const predicting = computed(() => predict.value && pathsMode.value)
const picks = ref(new Map<string, string>())
const active = ref<string | null>(null)
const revealed = ref(false)
const deal = ref(1)
const game = computed<Puzzle | null>(() => {
  if (!predicting.value || !center.value || !index.value || !scan.value?.calls) return null
  return puzzle(pathsTo(center.value, callersOf, 12, shown))
})
function restart(): void {
  picks.value = new Map()
  revealed.value = false
  deal.value = Math.floor(Math.random() * 2 ** 31)
  active.value = game.value?.slots[0]?.id ?? null
}
watch([center, predicting, () => game.value?.slots.map(slot => slot.id).join(' ')], restart, { immediate: true })
// The function panel lists callers, which would give the answer away: closed while a puzzle is open.
watch(predicting, (on) => {
  if (on && route.query.fn) void router.push({ query: { ...route.query, fn: undefined } })
}, { immediate: true })

function functionsOfParts(parts: Set<string>): string[] {
  const out: string[] = []
  const walk = (items: FunctionNode[]) => items.forEach((fn) => {
    out.push(fn.id)
    walk(fn.children)
  })
  for (const part of parts) for (const file of index.value?.filesByPart.get(part) ?? []) walk(file.functions)
  return out
}
const offered = computed(() => {
  const found = game.value
  const at = index.value
  if (!found || !at) return []
  const real = found.slots.map(slot => slot.real)
  const parts = new Set(real.map(id => at.functions.get(id)!.file.part))
  const ends = new Set(found.paths.flatMap(path => [path[0]!, path.at(-1)!]))
  return candidates(real, functionsOfParts(parts).filter(id => shown(id) && !ends.has(id)), deal.value)
})
function fill(id: string): void {
  const found = game.value
  if (!found || !active.value || revealed.value) return
  picks.value = new Map(picks.value).set(active.value, id)
  const at = found.slots.findIndex(slot => slot.id === active.value)
  const after = [...found.slots.slice(at + 1), ...found.slots.slice(0, at)]
  active.value = after.find(slot => !picks.value.has(slot.id))?.id ?? active.value
}
function clearPick(slot: string): void {
  const next = new Map(picks.value)
  next.delete(slot)
  picks.value = next
  active.value = slot
}
const marks = computed(() => (revealed.value && game.value ? grade(game.value, picks.value, (from, to) => calleesOf(from).includes(to)) : null))
const right = computed(() => [...marks.value?.values() ?? []].filter(Boolean).length)
function revealPaths(): void {
  revealed.value = true
  active.value = null
  const known = [...marks.value ?? []].filter(([, ok]) => ok).map(([slot]) => picks.value.get(slot)!)
  if (known.length) void mark(known, 'call-predict')
}
const activeNumber = computed(() => game.value?.slots.find(slot => slot.id === active.value)?.number ?? null)

/** The puzzle as cards: the entry points and the function in their files' cards, each place a card of its own. */
const predictGraph = computed<Graph | null>(() => {
  const found = game.value
  const at = index.value
  if (!found || !at || !center.value) return null
  const ends = [...new Set(found.paths.flatMap(path => [path[0]!, path.at(-1)!]))]
  const base = pathGraph(at, ends, [], center.value, { ...options.value, imports: null })
  const cards: GraphCard[] = [...base.cards]
  const edges: GraphEdge[] = []
  for (const slot of found.slots) cards.push({ id: `slot-card:${slot.id}`, kind: 'slot', name: '', rows: [{ id: `slot:${slot.id}`, kind: 'slot', depth: 0, context: false }], folded: 0, shut: false, open: false, home: false })
  found.paths.forEach((path, column) => {
    const ids = path.map((id, place) => {
      if (place === 0 || place === path.length - 1) return id
      return `slot:${found.slots.find(item => item.path === column && item.at === place)!.id}`
    })
    for (let place = 1; place < ids.length; place++) {
      const id = `${ids[place - 1]}->${ids[place]}`
      if (!edges.some(edge => edge.id === id)) edges.push({ id, from: ids[place - 1]!, to: ids[place]!, kind: 'call', count: 1, guessed: false, typeOnly: false, tone: 'plain' })
    }
  })
  return { cards, edges, inner: [], leftOut: 0 }
})
function slotLook(rowId: string): SlotLook {
  const slot = game.value?.slots.find(item => `slot:${item.id}` === rowId)
  if (!slot) return { label: '?', tone: 'empty' }
  const pick = picks.value.get(slot.id)
  if (!revealed.value) {
    if (slot.id === active.value) return { label: pick ? `${slot.number}. ${name(pick)}` : `${slot.number}  ?`, tone: 'active' }
    return pick ? { label: `${slot.number}. ${name(pick)}`, tone: 'picked' } : { label: `${slot.number}  ?`, tone: 'empty' }
  }
  if (marks.value?.get(slot.id)) return { label: `${slot.number}. ${name(pick!)} ✓`, tone: 'right' }
  return pick ? { label: `${slot.number}. ${name(pick)} ✗ ${name(slot.real)}`, tone: 'wrong' } : { label: `${slot.number}. ${name(slot.real)}`, tone: 'missed' }
}
const entries = computed(() => new Set(predicting.value ? (game.value?.paths.map(path => path[0]!) ?? []) : (paths.value?.entries ?? [])))

/* Clicks */
function pickRow(id: string): void {
  if (id.startsWith('slot:')) {
    if (!revealed.value) active.value = id.slice(5)
    return
  }
  if (index.value?.functions.has(id)) select(id)
  else go({ kind: 'file', path: id })
}
function openRow(id: string): void {
  if (index.value?.functions.has(id)) recenter(id)
}
function header(card: GraphCard): void {
  if (card.kind === 'idle' || card.kind === 'slot') return
  if (card.kind === 'file') go({ kind: 'file', path: card.name })
  else if (card.kind === 'part' || card.kind === 'more') go({ kind: 'part', name: card.name })
}
function toggle(which: 'opened' | 'flipped', id: string): void {
  const set = which === 'opened' ? opened : flipped
  const next = new Set(set.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  set.value = next
}
function fold(card: GraphCard): void {
  if (card.kind === 'part') go({ kind: 'part', name: card.name })
  else toggle('opened', card.id)
}
/** A function from Start from: fills the chosen place while predicting, else goes in the middle. */
function choose(id: string): void {
  starting.value = false
  if (predicting.value && active.value && !revealed.value) fill(id)
  else recenter(id)
}
const focusCard = computed(() => (focus.value.kind === 'file' ? `file:${focus.value.path}` : null))
const waiting = computed(() => focus.value.kind !== 'project' && !scan.value?.calls)

/** Start from: the most called functions, the entry points, the recent ones and the parts, in a menu. */
const starting = ref(false)
// Typing in the toolbar's search opens it on what matches.
watch(term, (now) => {
  if (now) starting.value = true
})
function startFile(path: string): void {
  starting.value = false
  go({ kind: 'file', path })
}

/** What the graph shows besides the calls, ticked in one menu. */
const showItems = computed(() => {
  const tick = (label: string, checked: boolean, set: (value: boolean) => void, help?: string) => ({ label, description: help, type: 'checkbox' as const, checked, onUpdateChecked: set, onSelect: (event: Event) => event.preventDefault() })
  const items = [tick(t('pages.graph.includeTests'), includeTests.value, (value) => { includeTests.value = value }, t('pages.graph.includeTestsHelp'))]
  if (focus.value.kind === 'project') {
    items.push(tick(t('pages.graph.typeOnly'), typeOnly.value, (value) => { typeOnly.value = value }))
    if (rules.value) items.push(tick(t('pages.graph.onlyBreaks'), onlyBreaks.value, (value) => { onlyBreaks.value = value }))
  } else {
    items.push(tick(t('pages.graph.imports'), withImports.value, (value) => { withImports.value = value }, t('pages.graph.importsHelp')))
    items.push(tick(t('pages.graph.familiarity'), familiarity.value, (value) => { familiarity.value = value }, t('pages.graph.familiarityHelp')))
  }
  items.push(tick(t('pages.graph.hotspots'), hotspots.value, (value) => { hotspots.value = value }, t('pages.graph.hotspotsHelp', { count: days })))
  return [items]
})
/* Export: the whole graph as it is drawn, in the theme's colours, as a PNG or an SVG. */
const board = useTemplateRef<{ picture: () => { cards: SvgGraphCard[], lines: SvgGraphLine[] } }>('board')
const { repo } = useRepositories()
const toast = useToast()
const failure = useFailure()
const exporting = ref(false)
async function exportAs(format: ImageFormat): Promise<void> {
  const drawn = board.value?.picture()
  if (!drawn?.cards.length || exporting.value) return
  exporting.value = true
  try {
    const svg = graphSvg(drawn.cards, drawn.lines, { colors: themeColors(), maxWidth: 4000 })
    const file = `${repo.value ? folderName(repo.value) : 'haunt'}-graph.${format}`
    const path = await saveImage(svg, format, file, t('pages.graph.exportTitle'), t(`pages.graph.${format}`))
    if (path) toast.add({ title: t('pages.graph.exported'), description: path, icon: 'i-lucide-image', color: 'success' })
  } catch (error) {
    failure.failed(t('pages.graph.notExported'), error)
  } finally {
    exporting.value = false
  }
}
const exportItems = computed(() => [[
  { label: t('pages.graph.png'), icon: 'i-lucide-image', onSelect: () => { void exportAs('png') } },
  { label: t('pages.graph.svg'), icon: 'i-lucide-file-code', onSelect: () => { void exportAs('svg') } }
]])
/** Who knows a part (PartKnowledge: git blame of its files), read only when asked for. */
const knowing = ref(false)
const partPaths = computed(() => (focus.value.kind === 'part' ? (index.value?.filesByPart.get(focus.value.name) ?? []).filter(visible).map(file => file.path) : []))
watch(() => focusKey(focus.value), () => {
  knowing.value = false
})
const reachLabel = computed(() => (pathsMode.value ? t('pages.graph.reachBelow', { count: below.value }) : t(`pages.graph.reach.${direction.value}`, { count: depth.value })))
</script>

<template>
  <div v-if="!scan" class="grid justify-items-center gap-2 py-16 text-muted">
    <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
    <p>{{ t('pages.graph.scanning') }}</p>
  </div>
  <div v-else class="flex h-full min-w-0 flex-col">
    <div class="flex items-center gap-2 border-b border-default px-3 py-1.5">
      <UButton icon="i-lucide-arrow-left" size="xs" color="neutral" variant="ghost" :disabled="focus.kind === 'project'" :aria-label="t('pages.graph.back')" :title="t('pages.graph.back')" @click="back" />
      <nav class="flex min-w-0 flex-1 items-center gap-1 text-sm" :aria-label="t('pages.graph.trail')">
        <button type="button" class="shrink-0" :class="focus.kind === 'project' ? 'font-medium text-highlighted' : 'text-muted hover:text-default'" @click="go({ kind: 'project' }, false)">{{ t('pages.graph.project') }}</button>
        <template v-if="focus.kind !== 'project'">
          <template v-for="key in crumbs" :key="key">
            <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-dimmed" />
            <button type="button" class="flex min-w-0 items-center gap-1 font-mono" :class="key === focusKey(focus) ? 'font-medium text-highlighted' : 'text-muted hover:text-default'" @click="goKey(key)">
              <UIcon :name="crumbIcon(key)" class="size-3.5 shrink-0 text-dimmed" /><span class="truncate">{{ crumbLabel(key) }}</span>
            </button>
          </template>
        </template>
        <span v-if="summary" class="ml-2 hidden min-w-0 truncate text-xs text-muted xl:inline" :title="summary">{{ summary }}</span>
      </nav>

      <UPopover v-model:open="starting" :content="{ align: 'end' }">
        <UButton icon="i-lucide-crosshair" :label="t('pages.graph.startFrom')" size="xs" color="neutral" variant="outline" trailing-icon="i-lucide-chevron-down" />
        <template #content>
          <div class="flex max-h-[70vh] w-80 flex-col">
            <div v-if="!term" class="flex gap-0.5 border-b border-default p-1.5">
              <button
                v-for="item in tabs"
                :key="item"
                type="button"
                class="flex-1 rounded px-1 py-1 text-xs"
                :class="tab === item ? 'bg-elevated font-medium text-highlighted' : 'text-muted hover:bg-elevated/60'"
                @click="tab = item"
              >
                {{ t(`pages.graph.tab.${item}`) }}
              </button>
            </div>
            <p v-else class="border-b border-default px-3 py-2 text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.graph.matching') }}</p>
            <p v-if="!scan.calls" class="p-3 text-xs text-muted">{{ t('pages.graph.waiting') }}</p>
            <p v-else-if="tab === 'entries' && !term" class="px-3 pt-2 text-label text-muted">{{ t('pages.graph.entriesHelp') }}</p>
            <p v-else-if="tab === 'recent' && !term && !choices.length" class="p-3 text-xs text-muted">{{ t('pages.graph.recentEmpty') }}</p>
            <ul class="grid min-h-0 flex-1 content-start gap-0.5 overflow-auto p-1.5">
              <li v-for="choice in choices" :key="choice.id">
                <button type="button" class="grid w-full rounded px-2 py-1 text-left" :class="choice.id === center ? 'bg-elevated' : 'hover:bg-elevated/60'" @click="choose(choice.id)">
                  <span class="truncate font-mono text-xs text-highlighted">{{ choice.found.fn.name }}</span>
                  <span class="truncate font-mono text-label text-muted">
                    {{ tailOf(choice.found.file.path) }}<template v-if="scan.calls"> · {{ tab === 'entries' && !term ? t('pages.graph.callsCount', { count: choice.callees }) : t('pages.graph.calledFrom', { count: choice.callers }) }}</template>
                  </span>
                </button>
              </li>
              <template v-if="fileChoices.length">
                <li class="px-2 pt-2 text-label font-semibold tracking-wider text-muted uppercase">{{ t('pages.graph.matchingFiles') }}</li>
                <li v-for="file in fileChoices" :key="file.path">
                  <button type="button" class="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-elevated/60" :title="file.path" @click="startFile(file.path)">
                    <UIcon name="i-lucide-file" class="size-3.5 shrink-0 text-muted" />
                    <span class="truncate font-mono text-xs">{{ file.path }}</span>
                  </button>
                </li>
              </template>
            </ul>
          </div>
        </template>
      </UPopover>

      <template v-if="focus.kind === 'file'">
        <UButton :icon="bookmarks.isBookmarked(focus.path) ? 'i-lucide-bookmark-check' : 'i-lucide-bookmark'" size="xs" :color="bookmarks.isBookmarked(focus.path) ? 'primary' : 'neutral'" variant="ghost" :aria-label="bookmarks.isBookmarked(focus.path) ? t('pages.graph.unbookmark') : t('pages.graph.bookmark')" :title="bookmarks.isBookmarked(focus.path) ? t('pages.graph.unbookmark') : t('pages.graph.bookmark')" @click="bookmarks.toggle(focus.path)" />
        <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" :aria-label="t('pages.graph.openFile')" :title="t('pages.graph.openFile')" @click="open(focus.path)" />
      </template>
      <UButton v-if="focus.kind === 'part'" icon="i-lucide-users" :label="t('pages.graph.whoKnows')" size="xs" :color="knowing ? 'primary' : 'neutral'" :variant="knowing ? 'soft' : 'ghost'" @click="knowing = !knowing" />
      <template v-if="focus.kind === 'function'">
        <UFieldGroup size="xs">
          <UButton :label="t('pages.graph.around')" :color="mode === 'around' ? 'primary' : 'neutral'" :variant="mode === 'around' ? 'solid' : 'outline'" :title="t('pages.graph.aroundHelp')" @click="mode = 'around'" />
          <UButton :label="t('pages.graph.paths')" :color="mode === 'paths' ? 'primary' : 'neutral'" :variant="mode === 'paths' ? 'solid' : 'outline'" :title="t('pages.graph.pathsHelp')" @click="mode = 'paths'" />
        </UFieldGroup>
        <UButton v-if="pathsMode" icon="i-lucide-brain" :label="t('pages.graph.predict')" size="xs" :color="predict ? 'primary' : 'neutral'" :variant="predict ? 'soft' : 'ghost'" :title="t('pages.graph.predictHelp')" @click="predict = !predict" />
      </template>
      <UPopover v-if="focus.kind !== 'project'" :content="{ align: 'end' }">
        <UButton icon="i-lucide-route" :label="reachLabel" size="xs" color="neutral" variant="ghost" trailing-icon="i-lucide-chevron-down" />
        <template #content>
          <div class="grid gap-3 p-3 text-xs">
            <USelect v-if="!pathsMode" v-model="direction" :items="directions" size="xs" class="w-48" :aria-label="t('pages.graph.show')" />
            <div class="flex items-center justify-between gap-3 text-muted">
              {{ pathsMode ? t('pages.graph.stepsBelow') : t('pages.graph.stepsLabel') }}
              <UFieldGroup size="xs">
                <template v-if="pathsMode">
                  <UButton v-for="step in [1, 2]" :key="step" :label="String(step)" :color="below === step ? 'primary' : 'neutral'" :variant="below === step ? 'solid' : 'outline'" @click="below = step" />
                </template>
                <template v-else>
                  <UButton v-for="step in [1, 2]" :key="step" :label="String(step)" :color="depth === step ? 'primary' : 'neutral'" :variant="depth === step ? 'solid' : 'outline'" @click="depth = step" />
                </template>
              </UFieldGroup>
            </div>
          </div>
        </template>
      </UPopover>
      <template v-if="focus.kind === 'project'">
        <UButton v-if="rules" :label="t('pages.graph.overTime')" icon="i-lucide-chart-column" size="xs" :color="overTime ? 'primary' : 'neutral'" :variant="overTime ? 'soft' : 'ghost'" @click="overTime = !overTime" />
        <UPopover :content="{ align: 'end' }">
          <UButton icon="i-lucide-info" size="xs" color="neutral" variant="ghost" :aria-label="t('pages.graph.rules')" :title="t('pages.graph.rules')" />
          <template #content>
            <div class="grid max-w-md gap-2 p-3 text-sm">
              <p v-if="rules === 'config'" class="text-muted">
                {{ t('pages.graph.config.before') }} <code>haunt.config.json</code>{{ t('pages.graph.config.afterConfig') }}
                <code>uses</code>{{ t('pages.graph.config.afterUses') }} <code>entries</code>{{ t('pages.graph.config.after') }}
              </p>
              <p v-else-if="rules === 'script'" class="text-muted">{{ t('pages.graph.script.before') }} <code>haunt.config.json</code> {{ t('pages.graph.script.after') }}</p>
              <p v-else class="text-muted">{{ t('pages.graph.none.before') }} <code>haunt.config.json</code> {{ t('pages.graph.none.after') }}</p>
              <template v-if="scan.cycles.length">
                <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.graph.runtimeCycles') }}</h3>
                <ul class="grid max-h-60 gap-1.5 overflow-auto">
                  <li v-for="(cycle, i) in scan.cycles" :key="i" class="font-mono text-xs break-all text-error">{{ cycle.join(' → ') }}</li>
                </ul>
              </template>
            </div>
          </template>
        </UPopover>
      </template>
      <UDropdownMenu :items="exportItems" :content="{ align: 'end' }">
        <UButton icon="i-lucide-download" :aria-label="t('pages.graph.export')" :title="t('pages.graph.export')" size="xs" color="neutral" variant="ghost" :loading="exporting" :disabled="!graph?.cards.length" />
      </UDropdownMenu>
      <UDropdownMenu :items="showItems" :content="{ align: 'end' }" :ui="{ content: 'w-72' }">
        <UButton icon="i-lucide-sliders-horizontal" :label="t('pages.graph.showMenu')" size="xs" color="neutral" variant="ghost" trailing-icon="i-lucide-chevron-down" />
      </UDropdownMenu>
    </div>

    <div v-if="predicting && game && center" class="grid gap-2 border-b border-default px-3 py-2 text-xs">
      <p v-if="!game.slots.length" class="text-muted">{{ t('pages.graph.predictNothing') }}</p>
      <template v-else>
        <div class="flex flex-wrap items-center gap-2">
          <span v-if="revealed" class="font-medium text-highlighted tabular-nums">{{ t('pages.graph.predictScore', { right, count: game.slots.length }) }}</span>
          <span v-else-if="activeNumber" class="text-muted">{{ t('pages.graph.predictFill', { number: activeNumber }) }}</span>
          <span v-else class="text-muted">{{ t('pages.graph.predictDone') }}</span>
          <span class="flex-1" />
          <UButton v-if="!revealed" icon="i-lucide-eye" :label="t('pages.graph.reveal')" size="xs" color="primary" variant="soft" :disabled="!picks.size" @click="revealPaths" />
          <UButton icon="i-lucide-rotate-ccw" :label="revealed ? t('pages.graph.predictAgain') : t('pages.graph.predictClear')" size="xs" color="neutral" variant="ghost" @click="restart" />
        </div>
        <div v-if="!revealed" class="flex flex-wrap gap-1">
          <button
            v-for="id in offered"
            :key="id"
            type="button"
            class="rounded border px-2 py-0.5 font-mono"
            :class="[...picks.values()].includes(id) ? 'border-accented bg-elevated text-muted' : 'border-default hover:bg-elevated'"
            :disabled="!active"
            @click="fill(id)"
          >
            {{ name(id) }}
          </button>
        </div>
        <div v-if="!revealed && picks.size" class="flex flex-wrap gap-1 text-muted">
          <button v-for="slot in game.slots.filter(item => picks.has(item.id))" :key="slot.id" type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated" :title="t('pages.graph.predictUnpick')" @click="clearPick(slot.id)">
            {{ slot.number }}. <span class="font-mono text-default">{{ name(picks.get(slot.id)!) }}</span><UIcon name="i-lucide-x" class="size-3" />
          </button>
        </div>
      </template>
    </div>

    <ArchitectureHistory v-if="focus.kind === 'project' && overTime && rules" :now="allBreaks" />
    <div v-if="focus.kind === 'part' && knowing" class="max-h-[40vh] overflow-auto border-b border-default px-3 py-2">
      <PartKnowledge :paths="partPaths" />
    </div>
    <div v-if="focus.kind === 'project'" class="px-3 empty:hidden [&:has(*)]:pt-2">
      <TrustLayersNote />
      <UAlert v-if="scan.architectureError" class="mt-2" color="warning" variant="subtle" icon="i-lucide-triangle-alert" :title="t('pages.graph.rulesError')" :description="scan.architectureError" />
    </div>

    <div class="relative min-h-0 flex-1">
      <p v-if="waiting" class="absolute inset-0 grid place-items-center text-muted">{{ t('pages.graph.working') }}</p>
      <p v-else-if="focus.kind === 'function' && !center" class="absolute inset-0 grid place-items-center px-6 text-center text-muted">{{ t('pages.graph.gone') }}</p>
      <p v-else-if="graph && !graph.cards.length" class="absolute inset-0 grid place-items-center px-6 text-center text-muted">{{ t('pages.graph.empty') }}</p>
      <CodeGraph
        v-else-if="graph"
        ref="board"
        :graph="graph"
        :places="places"
        :center="center ?? focusCard"
        :selected="selected?.fn.id"
        :term="search"
        :entries="entries"
        :familiar="familiarity ? states : null"
        :heat="heat"
        :slot="slotLook"
        @row="pickRow"
        @open="openRow"
        @header="header"
        @fold="fold"
        @shut="card => toggle('flipped', card.id)"
        @background="select(null)"
      >
        <template #legend>
          <template v-if="focus.kind === 'project'">
            <span class="tabular-nums">{{ t('pages.graph.importsCount', { count: importSummary.imports }) }}</span>
            <template v-if="rules">
              <span class="flex items-center gap-1.5 tabular-nums"><i class="inline-block h-0.5 w-4 bg-(--rule-new)" />{{ importSummary.known ? t('pages.graph.newBreaks', { count: importSummary.new }) : t('pages.graph.breaksCount', { count: importSummary.new }) }}</span>
              <span v-if="importSummary.known" class="flex items-center gap-1.5 tabular-nums"><i class="inline-block h-0.5 w-4 bg-(--rule-known)" />{{ t('pages.graph.known', { count: importSummary.known }) }}</span>
            </template>
            <span class="tabular-nums" :class="scan.cycles.length ? 'text-error' : ''">{{ t('pages.graph.cycles', { count: scan.cycles.length }) }}</span>
          </template>
          <template v-else>
            <span v-if="changedById.size" class="flex items-center gap-1"><i class="h-2.5 w-0.75 bg-warning" />{{ t('pages.graph.changed') }}</span>
            <span v-if="changedById.size" class="flex items-center gap-1"><i class="h-2.5 w-0.75 bg-warning/45" />{{ t('pages.graph.affected') }}</span>
            <template v-if="familiarity">
              <span class="flex items-center gap-1"><i class="size-2 rounded-full border border-(--ui-text-dimmed)" />{{ t('pages.graph.notLearned') }}</span>
              <span class="flex items-center gap-1"><i class="size-2 rounded-full bg-success" />{{ t('pages.graph.understood') }}</span>
              <span class="flex items-center gap-1"><i class="size-2 rounded-full bg-warning" />{{ t('pages.graph.changedSince') }}</span>
            </template>
            <span v-if="withImports" class="flex items-center gap-1"><i class="inline-block h-0.5 w-4 bg-info" />{{ t('pages.graph.imports') }}</span>
          </template>
        </template>
      </CodeGraph>
      <UIcon v-if="placing" name="i-lucide-loader-circle" class="absolute top-3 right-3 size-4 animate-spin text-muted" />
    </div>
  </div>
</template>
