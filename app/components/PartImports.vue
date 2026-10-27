<script setup lang="ts">
import type { ElementDefinition, LayoutOptions } from 'cytoscape'
import { folderAt, folderTree, level, nodeAtLevel } from '~~/shared/folders'

/*
 * Inside a part: the folders and files one level inside a folder, with the imports between them as arrows as thick as
 * their number, laid out top to bottom by who imports whom. Arrows in a cycle (of runtime imports) are red. Double-click
 * a folder to go a level deeper, a file to see it in Layers; the breadcrumb goes back up. Test files are left out.
 */
const props = defineProps<{ part: string, folder: string }>()
const { scan, index, visible, open } = useCode()
const router = useRouter()
const colorMode = useColorMode()

const typeOnly = ref(true)
const picked = ref<{ kind: 'edge' | 'node', id: string } | null>(null)

const part = computed(() => index.value?.parts.get(props.part) ?? null)
const partFolder = computed(() => part.value?.folder ?? '')
const files = computed(() => (index.value?.filesByPart.get(props.part) ?? []).filter(file => visible(file) && !file.test))
const tree = computed(() => folderTree(files.value, partFolder.value))
const folder = computed(() => folderAt(tree.value, props.folder) ?? tree.value)
const inner = computed(() => {
  const known = new Set(files.value.map(file => file.path))
  return (scan.value?.innerImports ?? []).filter(item => known.has(item.from) && known.has(item.to) && (typeOnly.value || !item.typeOnly))
})
const shown = computed(() => level(folder.value, inner.value, partFolder.value))
const labels = computed(() => new Map(shown.value.nodes.map(node => [node.id, node.kind === 'folder' ? `${node.label}/` : node.label])))

const crumbs = computed(() => {
  const out = [{ label: props.part, path: '' }]
  let at = ''
  for (const segment of folder.value.path.split('/').filter(Boolean)) {
    at += `${segment}/`
    out.push({ label: segment, path: at })
  }
  return out
})
function go(to: string): void {
  picked.value = null
  void router.push({ query: { part: props.part, folder: to || undefined } })
}

const elements = computed<ElementDefinition[]>(() => {
  void colorMode.value
  const color = resolvedColor(layerVar(part.value?.layer ?? 'unmapped'))
  const out: ElementDefinition[] = shown.value.nodes.map((node) => {
    const label = node.kind === 'folder' ? `${node.label}/\n${t('PartImports.folderSize', { files: t('common.files', { count: node.files }), functions: t('common.fns', { count: node.functions }) })}` : node.label
    return { group: 'nodes', data: { id: node.id, label, color, ...nodeSize(label) }, classes: node.kind }
  })
  const plain = resolvedColor('--ui-text-dimmed')
  const red = resolvedColor('--rule-new')
  for (const edge of shown.value.edges) {
    out.push({
      group: 'edges',
      data: { id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to, width: Math.min(8, 1 + Math.log2(edge.count)), color: edge.cycle ? red : plain },
      classes: [edge.cycle ? 'new' : '', edge.typeOnly ? 'dashed' : ''].join(' ')
    })
  }
  return out
})
const layout: LayoutOptions = { name: 'dagre', rankDir: 'TB', nodeSep: 28, rankSep: 70, fit: true, padding: 40 } as LayoutOptions

function openNode(id: string): void {
  const node = shown.value.nodes.find(item => item.id === id)
  if (node?.kind === 'folder') go(node.id)
  else if (node) void router.push({ path: '/layers', query: { part: props.part, folder: folder.value.path || undefined } })
}

const pickedEdge = computed(() => {
  if (picked.value?.kind !== 'edge') return null
  const edge = shown.value.edges.find(item => `${item.from}->${item.to}` === picked.value!.id)
  if (!edge) return null
  const nodeOf = nodeAtLevel(folder.value, partFolder.value)
  return { edge, files: inner.value.filter(item => nodeOf(item.from) === edge.from && nodeOf(item.to) === edge.to) }
})
const pickedNode = computed(() => {
  if (picked.value?.kind !== 'node') return null
  const id = picked.value.id
  return {
    node: shown.value.nodes.find(item => item.id === id),
    out: shown.value.edges.filter(edge => edge.from === id).sort((a, b) => b.count - a.count),
    in: shown.value.edges.filter(edge => edge.to === id).sort((a, b) => b.count - a.count)
  }
})
const cycles = computed(() => shown.value.edges.filter(edge => edge.cycle).length)
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-default px-4 py-2 text-sm">
      <nav class="flex flex-wrap items-center gap-1 font-mono text-xs" :aria-label="t('PartImports.folder')">
        <button type="button" class="text-muted hover:text-highlighted" @click="router.push({ query: {} })">{{ t('PartImports.allParts') }}</button>
        <template v-for="(crumb, at) in crumbs" :key="crumb.path">
          <UIcon name="i-lucide-chevron-right" class="size-3.5 text-muted" />
          <button v-if="at < crumbs.length - 1" type="button" class="text-muted hover:text-highlighted" @click="go(crumb.path)">{{ crumb.label }}</button>
          <span v-else class="text-highlighted">{{ crumb.label }}</span>
        </template>
      </nav>
      <span class="text-muted tabular-nums">{{ t('PartImports.summary', { count: shown.edges.reduce((sum, edge) => sum + edge.count, 0), nodes: shown.nodes.length }) }}</span>
      <span v-if="cycles" class="text-error tabular-nums">{{ t('PartImports.cycles', { count: cycles }) }}</span>
      <USwitch v-model="typeOnly" :label="t('PartImports.typeOnly')" size="sm" class="ml-auto" />
    </div>
    <div class="flex min-h-0 flex-1">
      <div class="relative min-w-0 flex-1">
        <p v-if="scan && !scan.innerImports" class="absolute inset-0 grid place-items-center px-8 text-center text-sm text-muted">
          {{ t('PartImports.scriptOnly') }}
        </p>
        <p v-else-if="!shown.nodes.length" class="absolute inset-0 grid place-items-center text-sm text-muted">{{ t('PartImports.empty') }}</p>
        <GraphView
          v-else
          :elements="elements"
          :stylesheet="graphStyle"
          :layout="layout"
          :selected="picked?.id"
          @node="id => picked = { kind: 'node', id }"
          @edge="id => picked = { kind: 'edge', id }"
          @open="openNode"
          @background="picked = null"
        />
      </div>
      <aside class="w-96 shrink-0 overflow-auto border-l border-default p-4">
        <template v-if="pickedEdge">
          <div class="font-mono text-xs tracking-wider text-muted uppercase">{{ t('PartImports.imports') }}</div>
          <h2 class="mb-3 font-mono text-sm font-semibold break-all text-highlighted">{{ labels.get(pickedEdge.edge.from) }} → {{ labels.get(pickedEdge.edge.to) }}</h2>
          <p v-if="pickedEdge.edge.cycle" class="mb-3 text-xs text-error">{{ t('PartImports.inCycle') }}</p>
          <ul class="grid gap-2">
            <li v-for="item in pickedEdge.files" :key="`${item.from}->${item.to}`" class="grid gap-1 rounded-md border border-default p-2">
              <button type="button" class="text-left font-mono text-xs break-all hover:underline" @click="open(item.from)">{{ item.from }}</button>
              <span class="font-mono text-xs break-all text-muted">→ {{ item.to }}</span>
              <UBadge v-if="item.typeOnly" :label="t('PartImports.typesOnly')" size="sm" variant="outline" color="neutral" class="justify-self-start" />
            </li>
          </ul>
        </template>
        <template v-else-if="pickedNode?.node">
          <div class="font-mono text-xs tracking-wider text-muted uppercase">{{ pickedNode.node.kind === 'folder' ? t('PartImports.folder') : t('PartImports.file') }}</div>
          <h2 class="mb-1 font-mono text-sm font-semibold break-all text-highlighted">{{ labels.get(pickedNode.node.id) }}</h2>
          <p class="mb-3 text-xs text-muted tabular-nums">{{ t('PartImports.nodeSize', { files: t('common.files', { count: pickedNode.node.files }), functions: t('common.functions', { count: pickedNode.node.functions }) }) }}</p>
          <UButton v-if="pickedNode.node.kind === 'folder'" :label="t('PartImports.inside')" icon="i-lucide-folder-open" size="sm" color="neutral" variant="outline" class="mb-4" @click="go(pickedNode.node.id)" />
          <section v-for="side in (['out', 'in'] as const)" :key="side" class="mb-4 grid gap-1.5">
            <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t(`PartImports.side.${side}`, { count: pickedNode[side].length }) }}</h3>
            <button
              v-for="edge in pickedNode[side]"
              :key="`${edge.from}->${edge.to}`"
              type="button"
              class="flex items-center justify-between rounded px-2 py-1 text-left font-mono text-xs hover:bg-elevated"
              @click="picked = { kind: 'edge', id: `${edge.from}->${edge.to}` }"
            >
              <span :class="{ 'text-error': edge.cycle }">{{ labels.get(side === 'out' ? edge.to : edge.from) }}</span>
              <span class="text-muted tabular-nums">{{ edge.count }}</span>
            </button>
          </section>
        </template>
        <template v-else>
          <h2 class="mb-2 font-semibold text-highlighted">{{ t('PartImports.title', { name: folder.path ? folder.name : props.part }) }}</h2>
          <p class="mb-3 text-sm text-muted">
            {{ t('PartImports.help') }}
          </p>
        </template>
      </aside>
    </div>
  </div>
</template>
