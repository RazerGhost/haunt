<script lang="ts">
/** The functions put in the middle, in order, for the trail (going back cuts it); kept while Haunt runs. */
const trail = ref<string[]>([])
/** Every function put in the middle, the latest first, for the Recent list. */
const recent = ref<string[]>([])
</script>

<script setup lang="ts">
import type { ElementDefinition } from 'cytoscape'
import { groupColumns } from '~~/shared/callGroups'
import { entryPoints, pathsTo, pathView } from '~~/shared/callPaths'
import { candidates, grade, puzzle, type Puzzle } from '~~/shared/predict'
import type { CallEdge, FunctionNode } from '~~/shared/types'
import { addTo } from '~~/shared/collect'

/*
 * The calls of one function, two ways. Paths (the default): how the code reaches it, top to bottom, from the entry
 * points (functions nothing in the scan calls: route handlers, exported APIs) down the shortest call paths to it, and
 * what it calls below it (shared/callPaths.ts). Around: its callers to the left and what it calls to the right, a
 * column per step, long columns in groups by part and folder (shared/callGroups.ts).
 *
 * The list on the left stays: the most called functions, the entry points, and the ones you looked at. The trail above
 * goes back. Click a function to see it in the panel, double-click to put it in the middle.
 */
const { scan, index, selected, search, select } = useCode()
const route = useRoute()
const router = useRouter()
const colorMode = useColorMode()
const { changedById, affected } = useChanges()
const { includeTests, isTestCode } = useTestSupport()

const mode = usePersisted<'paths' | 'around'>('calls-mode', 'paths', value => value === 'paths' || value === 'around')
const depth = ref(2)
/** How many steps of what it calls the path view shows below it. */
const below = ref(1)
const direction = ref<'both' | 'callers' | 'callees'>('both')
const directions = computed(() => (['both', 'callers', 'callees'] as const).map(value => ({ value, label: t(`pages.calls.direction.${value}`) })))
/** A safety limit: past this many functions in one column (three steps out of a much-called one), the rest are left out. */
const MAX_COLUMN = 400
/** The groups the viewer opened, by part and folder; a new function in the middle starts closed. */
const open = ref(new Set<string>())

const center = computed(() => {
  // From the function panel's Call graph button only the function is given.
  const id = typeof route.query.center === 'string' ? route.query.center : selected.value?.fn.id
  return id && index.value?.functions.has(id) ? id : null
})
const name = (id: string) => index.value?.functions.get(id)?.fn.name ?? id

function recenter(id: string | null): void {
  // While predicting, the panel (with the callers in it) stays closed.
  void router.push({ query: { ...route.query, center: id ?? undefined, fn: predicting.value ? undefined : id ?? undefined } })
}
// The trail follows the function in the middle: going back to one in it cuts what came after.
watch(center, (id) => {
  open.value = new Set()
  if (!id) return
  const at = trail.value.indexOf(id)
  trail.value = at >= 0 ? trail.value.slice(0, at + 1) : [...trail.value, id].slice(-30)
  recent.value = [id, ...recent.value.filter(other => other !== id)].slice(0, 30)
}, { immediate: true })
const crumbs = computed(() => trail.value.slice(-5))
function back(): void {
  const at = center.value ? trail.value.indexOf(center.value) : -1
  recenter(at > 0 ? trail.value[at - 1]! : null)
}

/* The list on the left */
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
/** How many functions call each one, test code left out unless included: counted once per call graph and setting. */
const callerCounts = computed(() => {
  const counts = new Map<string, number>()
  for (const [id, edges] of index.value?.callers ?? []) counts.set(id, edges.filter(edge => included(edge.from)).length)
  return counts
})
const callerCount = (id: string) => callerCounts.value.get(id) ?? 0
/** The functions shown, most called first: worked out once per scan and setting, not on every keystroke. */
const mostCalled = computed(() => [...index.value?.functions.keys() ?? []].filter(shown).sort((a, b) => callerCount(b) - callerCount(a)))
const choices = computed(() => {
  const at = index.value
  if (!at) return []
  let ids: string[]
  if (term.value) {
    ids = mostCalled.value.filter(id => name(id).toLowerCase().includes(term.value))
  } else if (tab.value === 'entries') {
    ids = entryPoints(mostCalled.value, id => callersOf(id).filter(shown), id => calleesOf(id).filter(shown))
  } else if (tab.value === 'recent') {
    ids = recent.value
  } else {
    ids = mostCalled.value
  }
  return ids.slice(0, 60).map(id => ({ id, found: at.functions.get(id)!, callers: callerCount(id), callees: calleesOf(id).filter(included).length }))
})

/* Paths: how it is reached, top to bottom */
const ROW = 96
const COLUMN = 250
const paths = computed(() => {
  void colorMode.value
  const at = index.value
  if (!at || !center.value || !scan.value?.calls || mode.value !== 'paths') return null
  const view = pathView(center.value, callersOf, calleesOf, { down: below.value, keep: shown })
  const elements: ElementDefinition[] = []
  const rowLabel = (step: number) => step < 0 ? (step === -1 ? t('pages.calls.calledBy') : t('pages.calls.stepsUp', { count: -step })) : step === 0 ? '' : step === 1 ? t('pages.calls.calls') : t('pages.calls.stepsDown', { count: step })
  for (const row of view.rows) {
    if (row.step !== 0) {
      const label = rowLabel(row.step)
      elements.push({ group: 'nodes', data: { id: `row:${row.step}`, label, color: 'transparent', ...nodeSize(label) }, position: { x: -((row.ids.length - 1) / 2) * COLUMN - COLUMN * 0.8, y: row.step * ROW }, classes: 'rowlabel' })
    }
    row.ids.forEach((id, place) => {
      const located = at.functions.get(id)!
      const label = `${located.fn.name}\n${tailOf(located.file.path)}`
      const layer = at.parts.get(located.file.part)?.layer ?? 'unmapped'
      const entry = row.step < 0 && view.entries.includes(id)
      const classes = [id === center.value ? 'center' : '', entry ? 'entry' : '', changedById.value.has(id) ? 'changed' : affected.value.has(id) ? 'affected' : ''].join(' ')
      elements.push({
        group: 'nodes',
        data: { id, label, color: resolvedColor(layerVar(layer)), ...nodeSize(label, id === center.value ? 13 : 11) },
        position: { x: (place - (row.ids.length - 1) / 2) * COLUMN, y: row.step * ROW },
        classes
      })
    })
  }
  const line = resolvedColor('--ui-text-dimmed')
  for (const edge of view.edges) {
    const guessed = at.callees.get(edge.from)?.find(call => call.to === edge.to)?.guessed
    elements.push({ group: 'edges', data: { id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to, width: 1.5, color: line }, classes: guessed ? 'dashed' : '' })
  }
  return { elements, view }
})
const summary = computed(() => {
  const found = paths.value?.view
  if (!found || !center.value) return ''
  const who = name(center.value)
  const below = calleesOf(center.value).filter(shown).length
  const calls = below ? t('pages.calls.itCalls', { count: below }) : t('pages.calls.itCallsNothing')
  if (found.isEntry) return `${t('pages.calls.isEntry', { name: who })} ${calls}`
  if (found.cyclic) return `${t('pages.calls.inCycle', { name: who })} ${calls}`
  const entries = found.entries.length
  const steps = found.shortest === found.longest ? t('pages.calls.steps', { count: found.shortest }) : t('pages.calls.stepsRange', { shortest: found.shortest, longest: found.longest })
  const reached = t(found.paths >= 12 ? 'pages.calls.reachedShortest' : 'pages.calls.reached', { name: who, count: entries, steps })
  return `${reached} ${calls}`
})

/* Around: callers to the left, callees to the right */
function toggle(id: string): void {
  const key = id.split(':').slice(2).join(':')
  const next = new Set(open.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  open.value = next
}
const shortPart = (part: string) => part.split('/').pop() ?? part

const around = computed(() => {
  void colorMode.value
  const at = index.value
  if (!at || !center.value || !scan.value?.calls || mode.value !== 'around') return { elements: [] as ElementDefinition[], hidden: 0, grouped: 0 }
  const column = new Map<string, { column: number, weight: number }>([[center.value, { column: 0, weight: 0 }]])
  let hidden = 0
  const walk = (sign: 1 | -1, next: (id: string) => CallEdge[], other: (edge: CallEdge) => string) => {
    let frontier = [center.value!]
    for (let step = 1; step <= depth.value; step++) {
      const found = new Map<string, number>()
      for (const id of frontier) {
        for (const edge of next(id)) {
          const target = other(edge)
          const located = at.functions.get(target)
          if (column.has(target) || !located || !included(target)) continue
          found.set(target, (found.get(target) ?? 0) + edge.count)
        }
      }
      const ranked = [...found].sort((a, b) => b[1] - a[1])
      hidden += Math.max(0, ranked.length - MAX_COLUMN)
      const kept = ranked.slice(0, MAX_COLUMN)
      frontier = kept.map(([id]) => id)
      for (const [id, weight] of kept) column.set(id, { column: sign * step, weight })
    }
  }
  if (direction.value !== 'callers') walk(1, id => at.callees.get(id) ?? [], edge => edge.to)
  if (direction.value !== 'callees') walk(-1, id => at.callers.get(id) ?? [], edge => edge.from)

  const whereOf = (id: string) => {
    const file = at.functions.get(id)!.file
    return { part: file.part, partFolder: at.parts.get(file.part)?.folder ?? '', path: file.path }
  }
  const keep = new Set([center.value, ...selected.value ? [selected.value.fn.id] : []])
  const { shown: items, shownAs } = groupColumns(column, whereOf, open.value, keep)

  const columns = new Map<number, typeof items>()
  for (const item of items) addTo(columns, item.column, item)
  const elements: ElementDefinition[] = []
  for (const [x, list] of columns) {
    list.sort((a, b) => b.weight - a.weight)
    list.forEach((item, row) => {
      const first = at.functions.get(item.members[0]!)!
      const layer = at.parts.get(first.file.part)?.layer ?? 'unmapped'
      const label = item.kind === 'group'
        ? `${shortPart(item.part!)}${item.folder ? ` · ${item.folder}` : ''}\n${t('pages.calls.groupFunctions', { count: item.members.length })}`
        : `${first.fn.name}\n${tailOf(first.file.path)}`
      const changed = item.members.some(id => changedById.value.has(id))
      const touched = item.members.some(id => affected.value.has(id))
      elements.push({
        group: 'nodes',
        data: { id: item.id, label, color: resolvedColor(layerVar(layer)), ...nodeSize(label, item.id === center.value ? 13 : 11) },
        position: { x: x * 300, y: (row - (list.length - 1) / 2) * 58 },
        classes: [item.id === center.value ? 'center' : '', item.kind === 'group' ? 'group' : '', changed ? 'changed' : touched ? 'affected' : ''].join(' ')
      })
    })
  }
  // The calls between what is shown, added up where a group shows several functions.
  const edges = new Map<string, { source: string, target: string, count: number, guessed: boolean }>()
  for (const id of column.keys()) {
    for (const edge of at.callees.get(id) ?? []) {
      if (!column.has(edge.to)) continue
      const source = shownAs.get(id)!
      const target = shownAs.get(edge.to)!
      if (source === target) continue
      const key = `${source}->${target}`
      const known = edges.get(key) ?? { source, target, count: 0, guessed: true }
      known.count += edge.count
      known.guessed &&= !!edge.guessed
      edges.set(key, known)
    }
  }
  const line = resolvedColor('--ui-text-dimmed')
  for (const [id, edge] of edges) {
    // A call matched by name (PHP, Rust, Lua) is dashed: likely, not certain.
    elements.push({ group: 'edges', data: { id, source: edge.source, target: edge.target, width: Math.min(6, 1 + Math.log2(edge.count)), color: line }, classes: edge.guessed ? 'dashed' : '' })
  }
  const grouped = items.filter(item => item.kind === 'group').reduce((sum, item) => sum + item.members.length, 0)
  return { elements, hidden, grouped }
})

/*
 * Predict (Paths only): the entry points and the function stay, the functions between them are numbered places to
 * fill in (shared/predict.ts). Click a place, then a function from the list under the toolbar (the real ones among
 * decoys from the same parts) or from the list on the left, which the search box narrows to any function. Reveal shows
 * the real paths with each pick right or wrong; the right ones are marked familiar (useFamiliarity, `call-predict`).
 */
const predict = usePersisted('calls-predict', false, value => typeof value === 'boolean')
const predicting = computed(() => predict.value && mode.value === 'paths')
const picks = ref(new Map<string, string>())
const active = ref<string | null>(null)
const revealed = ref(false)
/** The seed the candidate list is dealt with; a new one with every new puzzle. */
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
// The function stays in the middle: only the panel closes.
watch(predicting, (on) => {
  if (on && selected.value) void router.push({ query: { ...route.query, center: center.value ?? undefined, fn: undefined } })
}, { immediate: true })

/** The functions of a set of parts, for decoys. */
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
  const pool = functionsOfParts(parts).filter(id => shown(id) && !ends.has(id))
  return candidates(real, pool, deal.value)
})
function fill(id: string): void {
  const found = game.value
  if (!found || !active.value || revealed.value) return
  picks.value = new Map(picks.value).set(active.value, id)
  // On to the next empty place after this one, round to the first.
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
const { mark } = useFamiliarity()
function revealPaths(): void {
  revealed.value = true
  active.value = null
  const known = [...marks.value ?? []].filter(([, ok]) => ok).map(([slot]) => picks.value.get(slot)!)
  if (known.length) void mark(known, 'call-predict')
}
const activeNumber = computed(() => game.value?.slots.find(slot => slot.id === active.value)?.number ?? null)

/** The puzzle drawn: a column per path, the entry point at the top, the function at the bottom, places between. */
const predictElements = computed(() => {
  void colorMode.value
  const found = game.value
  const at = index.value
  if (!found || !at || !center.value) return []
  const elements: ElementDefinition[] = []
  const muted = resolvedColor('--ui-text-muted')
  const colorOf = (id: string) => resolvedColor(layerVar(at.parts.get(at.functions.get(id)!.file.part)?.layer ?? 'unmapped'))
  const labelOf = (id: string) => `${name(id)}\n${tailOf(at.functions.get(id)!.file.path)}`
  const target = center.value
  const targetLabel = labelOf(target)
  elements.push({ group: 'nodes', data: { id: target, label: targetLabel, color: colorOf(target), ...nodeSize(targetLabel, 13) }, position: { x: 0, y: 0 }, classes: 'center' })
  const line = resolvedColor('--ui-text-dimmed')
  found.paths.forEach((path, column) => {
    const x = (column - (found.paths.length - 1) / 2) * COLUMN
    const ids = path.map((id, place) => {
      const y = (place - (path.length - 1)) * ROW
      if (place === path.length - 1) return target
      if (place === 0) {
        const label = labelOf(id)
        elements.push({ group: 'nodes', data: { id: `pe:${column}:${id}`, label, color: colorOf(id), ...nodeSize(label) }, position: { x, y }, classes: 'entry' })
        return `pe:${column}:${id}`
      }
      const slot = found.slots.find(item => item.path === column && item.at === place)!
      const pick = picks.value.get(slot.id)
      const ok = marks.value?.get(slot.id)
      let label: string
      let classes = 'slot'
      if (!revealed.value) {
        label = pick ? `${slot.number}. ${name(pick)}` : `${slot.number}  ?`
        if (slot.id === active.value) classes += ' active'
        if (pick) classes += ' picked'
      } else if (ok) {
        label = `${slot.number}. ${name(pick!)}`
        classes += ' right'
      } else {
        label = pick ? `${slot.number}. ${name(pick)} ✗\n${name(slot.real)}` : `${slot.number}. ${name(slot.real)}`
        classes += pick ? ' wrong' : ' missed'
      }
      // An empty place is as wide as a short name, to be seen and clicked.
      const size = nodeSize(label.length < 14 ? label.padEnd(14) : label)
      elements.push({ group: 'nodes', data: { id: `slot:${slot.id}`, label, color: revealed.value ? colorOf(slot.real) : muted, ...size }, position: { x, y }, classes })
      return `slot:${slot.id}`
    })
    for (let place = 1; place < ids.length; place++) {
      elements.push({ group: 'edges', data: { id: `${ids[place - 1]}->${ids[place]}`, source: ids[place - 1], target: ids[place], width: 1.5, color: line } })
    }
  })
  return elements
})

const elements = computed(() => (mode.value === 'paths' ? (predicting.value ? predictElements.value : paths.value?.elements ?? []) : around.value.elements))
/** A node's function: an entry point in Predict is drawn once per path (`pe:column:id`). */
const realId = (id: string) => (id.startsWith('pe:') ? id.split(':').slice(2).join(':') : id)
function pickNode(id: string): void {
  if (id.startsWith('group:')) toggle(id)
  else if (id.startsWith('slot:')) {
    if (!revealed.value) active.value = id.slice(5)
  } else if (!id.startsWith('row:')) select(realId(id))
}
function openNode(id: string): void {
  if (!id.startsWith('group:') && !id.startsWith('row:') && !id.startsWith('slot:')) recenter(realId(id))
}
/** A function in the list on the left: fills the chosen place while predicting, else goes in the middle. */
function choose(id: string): void {
  if (predicting.value && active.value && !revealed.value) fill(id)
  else recenter(id)
}
const layout = { name: 'preset' as const, fit: true, padding: 40 }
const graph = useTemplateRef<{ fit: () => void }>('graph')
</script>

<template>
  <div class="flex h-full">
    <aside class="flex w-72 shrink-0 flex-col border-r border-default">
      <div v-if="!term" class="flex gap-0.5 border-b border-default p-1.5">
        <button
          v-for="item in tabs"
          :key="item"
          type="button"
          class="flex-1 rounded px-1.5 py-1 text-xs"
          :class="tab === item ? 'bg-elevated font-medium text-highlighted' : 'text-muted hover:bg-elevated/60'"
          @click="tab = item"
        >
          {{ t(`pages.calls.tab.${item}`) }}
        </button>
      </div>
      <p v-else class="border-b border-default px-3 py-2 text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.calls.matching') }}</p>
      <p v-if="!scan?.calls" class="p-3 text-xs text-muted">{{ t('pages.calls.waiting') }}</p>
      <p v-else-if="tab === 'entries' && !term" class="px-3 pt-2 text-label text-muted">{{ t('pages.calls.entriesHelp') }}</p>
      <p v-else-if="tab === 'recent' && !term && !choices.length" class="p-3 text-xs text-muted">{{ t('pages.calls.recentEmpty') }}</p>
      <ul class="grid min-h-0 flex-1 content-start gap-0.5 overflow-auto p-1.5">
        <li v-for="choice in choices" :key="choice.id">
          <button
            type="button"
            class="grid w-full rounded px-2 py-1 text-left"
            :class="choice.id === center ? 'bg-elevated' : 'hover:bg-elevated/60'"
            @click="choose(choice.id)"
          >
            <span class="truncate font-mono text-xs text-highlighted">{{ choice.found.fn.name }}</span>
            <span class="truncate font-mono text-label text-muted">
              {{ tailOf(choice.found.file.path) }}<template v-if="scan?.calls"> · {{ tab === 'entries' && !term ? t('pages.calls.callsCount', { count: choice.callees }) : t('pages.calls.calledFrom', { count: choice.callers }) }}</template>
            </span>
          </button>
        </li>
      </ul>
    </aside>

    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex flex-wrap items-center gap-2 border-b border-default px-3 py-2">
        <UButton icon="i-lucide-arrow-left" size="xs" color="neutral" variant="ghost" :disabled="!center" :aria-label="t('pages.calls.back')" :title="t('pages.calls.back')" @click="back" />
        <nav class="flex min-w-0 items-center gap-1 text-sm" :aria-label="t('pages.calls.trail')">
          <button type="button" class="shrink-0 text-muted hover:text-default" @click="recenter(null)">{{ t('pages.calls.functions') }}</button>
          <template v-for="id in crumbs" :key="id">
            <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-dimmed" />
            <button type="button" class="min-w-0 truncate font-mono" :class="id === center ? 'font-medium text-highlighted' : 'text-muted hover:text-default'" @click="recenter(id)">{{ name(id) }}</button>
          </template>
        </nav>
        <div class="ml-auto flex items-center gap-2">
          <USwitch v-model="includeTests" :label="t('pages.calls.includeTests')" size="xs" :title="t('pages.calls.includeTestsHelp')" />
          <UFieldGroup size="xs">
            <UButton :label="t('pages.calls.paths')" :color="mode === 'paths' ? 'primary' : 'neutral'" :variant="mode === 'paths' ? 'solid' : 'outline'" :title="t('pages.calls.pathsHelp')" @click="mode = 'paths'" />
            <UButton :label="t('pages.calls.around')" :color="mode === 'around' ? 'primary' : 'neutral'" :variant="mode === 'around' ? 'solid' : 'outline'" :title="t('pages.calls.aroundHelp')" @click="mode = 'around'" />
          </UFieldGroup>
          <USwitch v-if="mode === 'paths'" v-model="predict" :label="t('pages.calls.predict')" size="xs" :title="t('pages.calls.predictHelp')" />
          <USelect v-if="mode === 'around'" v-model="direction" :items="directions" size="xs" class="w-44" :aria-label="t('pages.calls.show')" />
          <div class="flex items-center gap-1.5 text-xs text-muted">
            {{ mode === 'paths' ? t('pages.calls.stepsBelow') : t('pages.calls.stepsLabel') }}
            <UFieldGroup v-if="mode === 'paths'" size="xs">
              <UButton v-for="step in [1, 2]" :key="step" :label="String(step)" :color="below === step ? 'primary' : 'neutral'" :variant="below === step ? 'solid' : 'outline'" @click="below = step" />
            </UFieldGroup>
            <UFieldGroup v-else size="xs">
              <UButton v-for="step in [1, 2, 3]" :key="step" :label="String(step)" :color="depth === step ? 'primary' : 'neutral'" :variant="depth === step ? 'solid' : 'outline'" @click="depth = step" />
            </UFieldGroup>
          </div>
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-3 border-b border-default px-3 py-1.5 text-xs text-muted empty:hidden">
        <span v-if="mode === 'paths' && summary && !predicting">{{ summary }}</span>
        <template v-if="mode === 'around'">
          <span v-if="around.grouped">{{ t('pages.calls.grouped', { count: around.grouped }) }}</span>
          <span v-if="around.hidden">{{ t('pages.calls.hidden', { count: around.hidden, max: MAX_COLUMN }) }}</span>
        </template>
        <span v-if="changedById.size" class="flex items-center gap-3">
          <span class="flex items-center gap-1"><i class="size-2.5 rounded-sm bg-warning/60" />{{ t('pages.calls.changed') }}</span>
          <span class="flex items-center gap-1"><i class="size-2.5 rounded-sm border border-dashed border-warning" />{{ t('pages.calls.affected') }}</span>
        </span>
        <span v-if="center" class="ml-auto">{{ mode === 'paths' ? t('pages.calls.hintPaths') : t('pages.calls.hint') }}</span>
        <UButton v-if="center && scan?.calls" icon="i-lucide-maximize" :label="t('pages.calls.fit')" size="xs" color="neutral" variant="ghost" class="-my-1" @click="graph?.fit()" />
      </div>
      <div v-if="predicting && game && center" class="grid gap-2 border-b border-default px-3 py-2 text-xs">
        <p v-if="!game.slots.length" class="text-muted">{{ t('pages.calls.predictNothing') }}</p>
        <template v-else>
          <div class="flex flex-wrap items-center gap-2">
            <span v-if="revealed" class="font-medium text-highlighted tabular-nums">{{ t('pages.calls.predictScore', { right, count: game.slots.length }) }}</span>
            <span v-else-if="activeNumber" class="text-muted">{{ t('pages.calls.predictFill', { number: activeNumber }) }}</span>
            <span v-else class="text-muted">{{ t('pages.calls.predictDone') }}</span>
            <span class="flex-1" />
            <UButton v-if="!revealed" icon="i-lucide-eye" :label="t('pages.calls.reveal')" size="xs" color="primary" variant="soft" :disabled="!picks.size" @click="revealPaths" />
            <UButton icon="i-lucide-rotate-ccw" :label="revealed ? t('pages.calls.predictAgain') : t('pages.calls.predictClear')" size="xs" color="neutral" variant="ghost" @click="restart" />
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
            <button v-for="slot in game.slots.filter(item => picks.has(item.id))" :key="slot.id" type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-elevated" :title="t('pages.calls.predictUnpick')" @click="clearPick(slot.id)">
              {{ slot.number }}. <span class="font-mono text-default">{{ name(picks.get(slot.id)!) }}</span><UIcon name="i-lucide-x" class="size-3" />
            </button>
          </div>
        </template>
      </div>
      <div class="relative min-h-0 flex-1">
        <p v-if="!scan?.calls" class="absolute inset-0 grid place-items-center text-muted">{{ t('pages.calls.working') }}</p>
        <p v-else-if="!center" class="absolute inset-0 grid place-items-center px-6 text-center text-muted">{{ t('pages.calls.pick') }}</p>
        <GraphView
          v-else
          ref="graph"
          :elements="elements"
          :stylesheet="graphStyle"
          :layout="layout"
          :selected="selected?.fn.id"
          @node="pickNode"
          @open="openNode"
          @background="select(null)"
        />
      </div>
    </div>
  </div>
</template>
