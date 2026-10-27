<script setup lang="ts">
import type { ElementDefinition, LayoutOptions } from 'cytoscape'
import type { FileImport, PartImport } from '~~/shared/types'

/*
 * The imports between parts. With layer rules (the project's haunt.config.json, or the script it names) the parts sit
 * in a row per layer, top to bottom, and an arrow that breaks a rule is red, or amber when the project accepts it for
 * now (a script's `known`, such as a baseline). Without rules the parts are laid out top to bottom by who imports whom.
 * Double-click a part to go inside it (PartImports): its folders and the imports between them. Over time shows the
 * rule breaks each recent commit added and removed (ArchitectureHistory).
 */
const { structure: scan, open } = useCode()
const route = useRoute()
const router = useRouter()
const inPart = computed(() => (typeof route.query.part === 'string' ? route.query.part : null))
const inFolder = computed(() => (typeof route.query.folder === 'string' ? route.query.folder : ''))
const colorMode = useColorMode()

const typeOnly = ref(true)
const onlyBreaks = ref(false)
const picked = ref<{ kind: 'edge' | 'part', id: string } | null>(null)

const rules = computed(() => scan.value?.rules ?? null)
const ruled = computed(() => scan.value?.layers.filter(layer => layer.ruled) ?? [])

type Status = 'new' | 'known' | 'ok' | 'free'
function filesOf(group: PartImport): FileImport[] {
  return group.files.filter(file => typeOnly.value || !file.typeOnly)
}
function statusOf(files: FileImport[]): Status {
  if (!rules.value) return 'free'
  if (files.some(file => file.breaks.length && !file.known)) return 'new'
  return files.some(file => file.breaks.length) ? 'known' : 'ok'
}

const groups = computed(() => (scan.value?.imports ?? [])
  .map(group => ({ ...group, shown: filesOf(group) }))
  .filter(group => group.shown.length && (!onlyBreaks.value || !['ok', 'free'].includes(statusOf(group.shown)))))

/** The rule breaks there are now, type-only ones too, for the history worked back from it. */
const allBreaks = computed(() => (scan.value?.imports ?? []).flatMap(group => group.files).filter(file => file.breaks.length).length)
const overTime = ref(false)

const summary = computed(() => {
  const files = (scan.value?.imports ?? []).flatMap(filesOf)
  const breaks = files.filter(file => file.breaks.length)
  return { imports: files.length, known: breaks.filter(file => file.known).length, new: breaks.filter(file => !file.known).length }
})

function node(name: string, layer: string, position?: { x: number, y: number }): ElementDefinition {
  return { group: 'nodes', data: { id: name, label: name, color: resolvedColor(layerVar(layer)), ...nodeSize(name) }, ...position ? { position } : {} }
}

const elements = computed<ElementDefinition[]>(() => {
  void colorMode.value
  if (!scan.value) return []
  const out: ElementDefinition[] = []
  if (ruled.value.length) {
    // A row per layer of the rules; above them, the parts outside the rules that take part in an import.
    const inRules = new Set(ruled.value.map(layer => layer.id))
    const involved = new Set(groups.value.flatMap(group => [group.from, group.to]))
    const outside = scan.value.parts.filter(part => !inRules.has(part.layer) && involved.has(part.name))
    const rows = [
      ...outside.length ? [outside] : [],
      ...ruled.value.map(layer => scan.value!.parts.filter(part => part.layer === layer.id))
    ]
    rows.forEach((parts, row) => {
      const sorted = [...parts].sort((a, b) => a.name.localeCompare(b.name))
      sorted.forEach((part, column) => out.push(node(part.name, part.layer, { x: (column - (sorted.length - 1) / 2) * 150, y: row * 170 + (column % 2) * 40 })))
    })
  } else {
    for (const part of scan.value.parts) out.push(node(part.name, part.layer))
  }
  const present = new Set(out.map(item => item.data.id))
  const colors: Record<Status, string> = { new: resolvedColor('--rule-new'), known: resolvedColor('--rule-known'), ok: resolvedColor('--ui-text-dimmed'), free: resolvedColor('--ui-text-dimmed') }
  for (const group of groups.value) {
    if (!present.has(group.from) || !present.has(group.to)) continue
    const status = statusOf(group.shown)
    out.push({
      group: 'edges',
      data: { id: `${group.from}->${group.to}`, source: group.from, target: group.to, width: Math.min(7, 1 + Math.log2(group.shown.length)), color: colors[status] },
      classes: [status, group.shown.every(file => file.typeOnly) ? 'dashed' : ''].join(' ')
    })
  }
  return out
})

const layout = computed<LayoutOptions>(() => ruled.value.length
  ? { name: 'preset', fit: true, padding: 40 }
  : { name: 'dagre', rankDir: 'TB', nodeSep: 40, rankSep: 90, fit: true, padding: 40 } as LayoutOptions)

const pickedEdge = computed(() => {
  if (picked.value?.kind !== 'edge') return null
  return groups.value.find(group => `${group.from}->${group.to}` === picked.value!.id) ?? null
})
const pickedPart = computed(() => {
  if (picked.value?.kind !== 'part') return null
  const name = picked.value.id
  return {
    name,
    out: groups.value.filter(group => group.from === name).sort((a, b) => b.shown.length - a.shown.length),
    in: groups.value.filter(group => group.to === name).sort((a, b) => b.shown.length - a.shown.length)
  }
})
</script>

<template>
  <div v-if="inPart" class="flex h-full flex-col">
    <PartImports :part="inPart" :folder="inFolder" />
  </div>
  <div v-else class="flex h-full flex-col">
    <div class="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-default px-4 py-2 text-sm">
      <span class="text-muted tabular-nums">{{ t('pages.imports.summary', { count: summary.imports }) }}</span>
      <template v-if="rules">
        <span class="flex items-center gap-1.5 tabular-nums"><i class="inline-block h-0.5 w-5 bg-(--rule-new)" />{{ summary.known ? t('pages.imports.newBreaks', { count: summary.new }) : t('pages.imports.breaksCount', { count: summary.new }) }}</span>
        <span v-if="summary.known" class="flex items-center gap-1.5 tabular-nums"><i class="inline-block h-0.5 w-5 bg-(--rule-known)" />{{ t('pages.imports.known', { count: summary.known }) }}</span>
      </template>
      <span class="tabular-nums" :class="scan?.cycles.length ? 'text-error' : 'text-muted'">{{ t('pages.imports.cycles', { count: scan?.cycles.length ?? 0 }) }}</span>
      <div class="ml-auto flex flex-wrap items-center gap-4">
        <UButton v-if="rules" :label="t('pages.imports.overTime')" icon="i-lucide-chart-column" size="xs" :color="overTime ? 'primary' : 'neutral'" :variant="overTime ? 'soft' : 'ghost'" @click="overTime = !overTime" />
        <USwitch v-model="typeOnly" :label="t('pages.imports.typeOnly')" size="sm" />
        <USwitch v-if="rules" v-model="onlyBreaks" :label="t('pages.imports.onlyBreaks')" size="sm" />
      </div>
    </div>
    <ArchitectureHistory v-if="overTime && rules" :now="allBreaks" />
    <TrustLayersNote class="m-4 mb-0" />
    <UAlert v-if="scan?.architectureError" class="m-4" color="warning" variant="subtle" icon="i-lucide-triangle-alert" :title="t('pages.imports.rulesError')" :description="scan.architectureError" />
    <div class="flex min-h-0 flex-1">
      <div class="min-w-0 flex-1">
        <GraphView
          v-if="elements.length"
          :elements="elements"
          :stylesheet="graphStyle"
          :layout="layout"
          :selected="picked?.id"
          @node="id => picked = { kind: 'part', id }"
          @edge="id => picked = { kind: 'edge', id }"
          @open="id => router.push({ query: { part: id } })"
          @background="picked = null"
        />
        <p v-else-if="scan" class="grid h-full place-items-center px-6 text-center text-sm text-muted">{{ t('pages.imports.noParts') }}</p>
      </div>
      <aside class="w-96 shrink-0 overflow-auto border-l border-default p-4">
        <template v-if="pickedEdge">
          <div class="font-mono text-xs tracking-wider text-muted uppercase">{{ t('pages.imports.imports') }}</div>
          <h2 class="mb-3 font-mono text-base font-semibold text-highlighted">{{ pickedEdge.from }} → {{ pickedEdge.to }}</h2>
          <ul class="grid gap-2">
            <li v-for="file in pickedEdge.shown" :key="`${file.from}->${file.to}`" class="grid gap-1 rounded-md border border-default p-2">
              <button type="button" class="text-left font-mono text-xs break-all hover:underline" @click="open(file.from)">{{ file.from }}</button>
              <span class="font-mono text-xs break-all text-muted">→ {{ file.to }}</span>
              <span class="flex flex-wrap gap-1">
                <UBadge v-if="file.typeOnly" :label="t('pages.imports.typesOnly')" size="sm" variant="outline" color="neutral" />
                <UBadge v-for="rule in file.breaks" :key="rule" :label="t(`pages.imports.breaks.${rule}`)" size="sm" variant="soft" :color="file.known ? 'warning' : 'error'" />
                <UBadge v-if="file.breaks.length && file.known" :label="t('pages.imports.accepted')" size="sm" variant="outline" color="warning" />
              </span>
            </li>
          </ul>
        </template>
        <template v-else-if="pickedPart">
          <div class="font-mono text-xs tracking-wider text-muted uppercase">{{ t('pages.imports.part') }}</div>
          <h2 class="mb-1 font-mono text-base font-semibold text-highlighted">{{ pickedPart.name }}</h2>
          <div class="mb-4 flex flex-wrap gap-2">
            <UButton :label="t('pages.imports.inside')" icon="i-lucide-folder-open" size="sm" color="neutral" variant="outline" @click="router.push({ query: { part: pickedPart.name } })" />
            <UButton :label="t('pages.imports.filesAndFunctions')" icon="i-lucide-layers" size="sm" color="neutral" variant="outline" :to="{ path: '/layers', query: { part: pickedPart.name } }" />
          </div>
          <section v-for="side in (['out', 'in'] as const)" :key="side" class="mb-4 grid gap-1.5">
            <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t(`pages.imports.side.${side}`, { count: pickedPart[side].length }) }}</h3>
            <button
              v-for="group in pickedPart[side]"
              :key="`${group.from}->${group.to}`"
              type="button"
              class="flex items-center justify-between rounded px-2 py-1 text-left font-mono text-xs hover:bg-elevated"
              @click="picked = { kind: 'edge', id: `${group.from}->${group.to}` }"
            >
              <span :class="{ 'text-error': statusOf(group.shown) === 'new', 'text-warning': statusOf(group.shown) === 'known' }">{{ side === 'out' ? group.to : group.from }}</span>
              <span class="text-muted tabular-nums">{{ group.shown.length }}</span>
            </button>
          </section>
        </template>
        <template v-else>
          <h2 class="mb-2 font-semibold text-highlighted">{{ t('pages.imports.title') }}</h2>
          <p v-if="rules === 'config'" class="mb-3 text-sm text-muted">
            {{ t('pages.imports.config.before') }} <code>haunt.config.json</code>{{ t('pages.imports.config.afterConfig') }}
            <code>uses</code>{{ t('pages.imports.config.afterUses') }} <code>entries</code>{{ t('pages.imports.config.after') }}
          </p>
          <p v-else-if="rules === 'script'" class="mb-3 text-sm text-muted">
            {{ t('pages.imports.script.before') }} <code>haunt.config.json</code> {{ t('pages.imports.script.after') }}
          </p>
          <p v-else class="mb-3 text-sm text-muted">
            {{ t('pages.imports.none.before') }}
            <code>haunt.config.json</code> {{ t('pages.imports.none.after') }}
          </p>
          <p class="mb-3 text-sm text-muted">{{ t('pages.imports.help') }}</p>
          <template v-if="scan?.cycles.length">
            <h3 class="mb-1 text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.imports.runtimeCycles') }}</h3>
            <ul class="grid gap-2">
              <li v-for="(cycle, i) in scan.cycles" :key="i" class="font-mono text-xs break-all text-error">{{ cycle.join(' → ') }}</li>
            </ul>
          </template>
        </template>
      </aside>
    </div>
  </div>
</template>
