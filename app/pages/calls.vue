<script lang="ts">
/** The functions put in the middle, in order, for the trail (going back cuts it); kept while Haunt runs. */
const trail = ref<string[]>([])
/** Every function put in the middle, the latest first, for the Recent list. */
const recent = ref<string[]>([])
</script>

<script setup lang="ts">
import type { ElementDefinition } from 'cytoscape'
import { groupColumns } from '~~/shared/callGroups'
import { entryPoints, pathView } from '~~/shared/callPaths'
import type { CallEdge } from '~~/shared/types'
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
const { scan, index, selected, search, visible, select } = useCode()
const route = useRoute()
const router = useRouter()
const colorMode = useColorMode()
const { changedById, affected } = useChanges()

const mode = usePersisted<'paths' | 'around'>('calls-mode', 'paths', value => value === 'paths' || value === 'around')
const depth = ref(2)
/** How many steps of what it calls the path view shows below it. */
const below = ref(1)
const direction = ref<'both' | 'callers' | 'callees'>('both')
const directions = [
  { value: 'both', label: 'Callers and callees' },
  { value: 'callers', label: 'Callers' },
  { value: 'callees', label: 'Callees' }
]
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
  void router.push({ query: { ...route.query, center: id ?? undefined, fn: id ?? undefined } })
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
const tabs = [
  { value: 'called', label: 'Most called' },
  { value: 'entries', label: 'Entry points' },
  { value: 'recent', label: 'Recent' }
] as const
const term = computed(() => search.value.trim().toLowerCase())
const shown = (id: string) => {
  const found = index.value?.functions.get(id)
  return !!found && !found.fn.anonymous && visible(found.file)
}
const callersOf = (id: string) => (index.value?.callers.get(id) ?? []).map(edge => edge.from)
const calleesOf = (id: string) => (index.value?.callees.get(id) ?? []).map(edge => edge.to)
const callerCount = (id: string) => index.value?.callers.get(id)?.length ?? 0
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
  return ids.slice(0, 60).map(id => ({ id, found: at.functions.get(id)!, callers: callersOf(id).length, callees: calleesOf(id).length }))
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
  const rowLabel = (step: number) => step < 0 ? (step === -1 ? 'Called by' : `${-step} steps up`) : step === 0 ? '' : step === 1 ? 'Calls' : `${step} steps down`
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
  const calls = below ? ` It calls ${below} ${below === 1 ? 'function' : 'functions'}.` : ' It calls nothing in the scan.'
  if (found.isEntry) return `${who} is an entry point: nothing in the scan calls it.${calls}`
  if (found.cyclic) return `${who} is called only from inside a cycle (functions that call each other), never from an entry point; its direct callers are shown.${calls}`
  const entries = found.entries.length
  const steps = found.shortest === found.longest ? `${found.shortest} ${found.shortest === 1 ? 'step' : 'steps'}` : `${found.shortest} to ${found.longest} steps`
  return `${who} is reached from ${entries} ${entries === 1 ? 'entry point' : 'entry points'} in ${steps}${found.paths >= 12 ? ' (the 12 shortest paths shown)' : ''}.${calls}`
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
          if (column.has(target) || !located || !visible(located.file)) continue
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
        ? `${shortPart(item.part!)}${item.folder ? ` · ${item.folder}` : ''}\n${item.members.length} functions`
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

const elements = computed(() => (mode.value === 'paths' ? paths.value?.elements ?? [] : around.value.elements))
function pickNode(id: string): void {
  if (id.startsWith('group:')) toggle(id)
  else if (!id.startsWith('row:')) select(id)
}
function openNode(id: string): void {
  if (!id.startsWith('group:') && !id.startsWith('row:')) recenter(id)
}
const layout = { name: 'preset' as const, fit: true, padding: 40 }
</script>

<template>
  <div class="flex h-full">
    <aside class="flex w-72 shrink-0 flex-col border-r border-default">
      <div v-if="!term" class="flex gap-0.5 border-b border-default p-1.5">
        <button
          v-for="item in tabs"
          :key="item.value"
          type="button"
          class="flex-1 rounded px-1.5 py-1 text-xs"
          :class="tab === item.value ? 'bg-elevated font-medium text-highlighted' : 'text-muted hover:bg-elevated/60'"
          @click="tab = item.value"
        >
          {{ item.label }}
        </button>
      </div>
      <p v-else class="border-b border-default px-3 py-2 text-xs font-semibold tracking-wider text-muted uppercase">Matching functions</p>
      <p v-if="!scan?.calls" class="p-3 text-xs text-muted">Waiting for the call graph.</p>
      <p v-else-if="tab === 'entries' && !term" class="px-3 pt-2 text-label text-muted">Functions nothing calls that call others: where the code is entered.</p>
      <p v-else-if="tab === 'recent' && !term && !choices.length" class="p-3 text-xs text-muted">The functions you put in the middle show here.</p>
      <ul class="grid min-h-0 flex-1 content-start gap-0.5 overflow-auto p-1.5">
        <li v-for="choice in choices" :key="choice.id">
          <button
            type="button"
            class="grid w-full rounded px-2 py-1 text-left"
            :class="choice.id === center ? 'bg-elevated' : 'hover:bg-elevated/60'"
            @click="recenter(choice.id)"
          >
            <span class="truncate font-mono text-xs text-highlighted">{{ choice.found.fn.name }}</span>
            <span class="truncate font-mono text-label text-muted">
              {{ tailOf(choice.found.file.path) }}<template v-if="scan?.calls"> · {{ tab === 'entries' && !term ? `calls ${choice.callees}` : `called from ${choice.callers}` }}</template>
            </span>
          </button>
        </li>
      </ul>
    </aside>

    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex flex-wrap items-center gap-2 border-b border-default px-3 py-2">
        <UButton icon="i-lucide-arrow-left" size="xs" color="neutral" variant="ghost" :disabled="!center" aria-label="Back" title="Back" @click="back" />
        <nav class="flex min-w-0 items-center gap-1 text-sm" aria-label="Trail">
          <button type="button" class="shrink-0 text-muted hover:text-default" @click="recenter(null)">Functions</button>
          <template v-for="id in crumbs" :key="id">
            <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-dimmed" />
            <button type="button" class="min-w-0 truncate font-mono" :class="id === center ? 'font-medium text-highlighted' : 'text-muted hover:text-default'" @click="recenter(id)">{{ name(id) }}</button>
          </template>
        </nav>
        <div class="ml-auto flex items-center gap-2">
          <UFieldGroup size="xs">
            <UButton label="Paths" :color="mode === 'paths' ? 'primary' : 'neutral'" :variant="mode === 'paths' ? 'solid' : 'outline'" title="How the code reaches it, top to bottom" @click="mode = 'paths'" />
            <UButton label="Around" :color="mode === 'around' ? 'primary' : 'neutral'" :variant="mode === 'around' ? 'solid' : 'outline'" title="Callers to the left, what it calls to the right" @click="mode = 'around'" />
          </UFieldGroup>
          <USelect v-if="mode === 'around'" v-model="direction" :items="directions" size="xs" class="w-44" aria-label="Show" />
          <div class="flex items-center gap-1.5 text-xs text-muted">
            {{ mode === 'paths' ? 'Steps below' : 'Steps' }}
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
        <span v-if="mode === 'paths' && summary">{{ summary }}</span>
        <template v-if="mode === 'around'">
          <span v-if="around.grouped">{{ around.grouped }} functions grouped by part and folder: click a group to open it.</span>
          <span v-if="around.hidden">{{ around.hidden }} more left out: over {{ MAX_COLUMN }} in a column.</span>
        </template>
        <span v-if="changedById.size" class="flex items-center gap-3">
          <span class="flex items-center gap-1"><i class="size-2.5 rounded-sm bg-warning/60" />changed</span>
          <span class="flex items-center gap-1"><i class="size-2.5 rounded-sm border border-dashed border-warning" />calls a changed one</span>
        </span>
        <span v-if="center" class="ml-auto">{{ mode === 'paths' ? 'Double border: an entry point · ' : '' }}click to inspect · double-click to centre · scroll to zoom</span>
      </div>
      <div class="relative min-h-0 flex-1">
        <p v-if="!scan?.calls" class="absolute inset-0 grid place-items-center text-muted">Working out the call graph…</p>
        <p v-else-if="!center" class="absolute inset-0 grid place-items-center px-6 text-center text-muted">Pick or search for a function: Paths shows how the code reaches it from its entry points.</p>
        <GraphView
          v-else
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
