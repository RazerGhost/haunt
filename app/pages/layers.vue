<script setup lang="ts">
import { layersSvg, type SvgBand } from '~~/shared/exportSvg'
import type { Layer } from '~~/shared/types'

/*
 * The layer map: pick a part to see its files and functions, or search across all of them. In a wide window the map is
 * a sidebar and the part fills the rest; narrower, the map sits above the part.
 */
const { structure: scan, status, message, visible, search, selected } = useCode()
const { on: hotspots, parts: heat, days } = useHotspots()
const route = useRoute()
const { repo } = useRepositories()
const toast = useToast()
const failure = useFailure()
const router = useRouter()

const wide = ref(false)
const query = window.matchMedia('(min-width: 1280px)')
const follow = (event: MediaQueryListEvent) => { wide.value = event.matches }
onMounted(() => {
  wide.value = query.matches
  query.addEventListener('change', follow)
})
onBeforeUnmount(() => query.removeEventListener('change', follow))

const largest = computed(() => {
  const sizes = new Map<string, number>()
  for (const file of scan.value?.files ?? []) if (visible(file)) sizes.set(file.part, (sizes.get(file.part) ?? 0) + countFunctions(file.functions))
  return [...sizes].sort((a, b) => b[1] - a[1])[0]?.[0] ?? ''
})
const part = computed(() => {
  if (typeof route.query.part === 'string') return route.query.part
  // The part of the selected function, else the largest part.
  return selected.value?.file.part ?? largest.value
})
function choose(name: string) {
  search.value = ''
  // Another part opens on its own folder.
  void router.push({ query: { ...route.query, part: name, folder: undefined } })
}

const term = computed(() => search.value.trim().toLowerCase())
const found = computed(() => {
  if (!term.value || !scan.value) return []
  return scan.value.files.filter(file => visible(file) && matchesSearch(file.functions, term.value) > 0)
})
const hits = computed(() => found.value.reduce((sum, file) => sum + matchesSearch(file.functions, term.value), 0))

/* Export: the whole layer map as the stack shows it (the files it shows), in the theme's colours. */
interface StackTile { name: string, files: number, functions: number }
const stack = useTemplateRef<{ bands: { layer: Layer, tiles: StackTile[], total: number }[], short: (name: string) => string }>('stack')
const exporting = ref(false)
async function exportAs(format: ImageFormat): Promise<void> {
  const shown = stack.value
  if (!shown || exporting.value) return
  exporting.value = true
  try {
    const bands: SvgBand[] = shown.bands.map(band => ({
      layer: band.layer.id,
      label: band.layer.label,
      size: t('LayerStack.bandSize', { parts: t('common.parts', { count: band.tiles.length }), functions: t('common.fns', { count: band.total }) }),
      tiles: band.tiles.map(tile => ({ name: shown.short(tile.name), size: t('LayerStack.tileSize', { files: t('common.files', { count: tile.files }), functions: t('common.fns', { count: tile.functions }) }) }))
    }))
    const svg = layersSvg(bands, { width: 1200, colors: themeColors() })
    const name = `${repo.value ? folderName(repo.value) : 'haunt'}-layers.${format}`
    const path = await saveImage(svg, format, name, t('pages.layers.exportTitle'), t(`pages.layers.${format}`))
    if (path) toast.add({ title: t('pages.layers.exported'), description: path, icon: 'i-lucide-image', color: 'success' })
  } catch (error) {
    failure.failed(t('pages.layers.notExported'), error)
  } finally {
    exporting.value = false
  }
}
const exportItems = computed(() => [[
  { label: t('pages.layers.png'), icon: 'i-lucide-image', onSelect: () => { void exportAs('png') } },
  { label: t('pages.layers.svg'), icon: 'i-lucide-file-code', onSelect: () => { void exportAs('svg') } }
]])
</script>

<template>
  <div v-if="!scan" class="grid justify-items-center gap-2 py-16 text-muted">
    <UIcon v-if="status !== 'error'" name="i-lucide-loader-circle" class="size-5 animate-spin" />
    <p>{{ message || t('pages.layers.scanning') }}</p>
  </div>
  <div v-else :class="wide ? 'flex h-full' : 'grid gap-5 px-4 py-5'">
    <aside v-if="wide" class="flex w-72 shrink-0 flex-col border-r border-default">
      <div class="flex items-center justify-between border-b border-default px-3 py-2">
        <span class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.layers.layers') }}</span>
        <div class="flex items-center gap-1">
          <USwitch v-model="hotspots" :label="t('pages.layers.hotspots')" size="xs" :title="t('pages.layers.hotspotsHelp', { count: days })" />
          <UDropdownMenu :items="exportItems" :content="{ align: 'end' }">
            <UButton icon="i-lucide-download" size="xs" color="neutral" variant="ghost" :loading="exporting" :aria-label="t('pages.layers.export')" :title="t('pages.layers.export')" />
          </UDropdownMenu>
        </div>
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-2">
        <LayerStack ref="stack" compact :selected-part="term ? null : part" :heat="heat" @select="choose" />
      </div>
    </aside>
    <template v-else>
      <div class="flex items-center justify-end gap-3">
        <USwitch v-model="hotspots" :label="t('pages.layers.hotspots')" size="xs" :title="t('pages.layers.hotspotsHelp', { count: days })" />
        <UDropdownMenu :items="exportItems" :content="{ align: 'end' }">
          <UButton icon="i-lucide-download" :label="t('pages.layers.export')" size="xs" color="neutral" variant="ghost" :loading="exporting" />
        </UDropdownMenu>
      </div>
      <LayerStack ref="stack" :selected-part="term ? null : part" :heat="heat" @select="choose" />
    </template>

    <div :class="wide ? 'grid min-w-0 flex-1 content-start gap-5 overflow-auto p-5' : 'contents'">
      <TrustLayersNote />
      <UAlert v-if="scan.architectureError" color="warning" variant="subtle" icon="i-lucide-triangle-alert" :title="t('pages.layers.mapError')" :description="scan.architectureError" />
      <section v-if="term" class="grid gap-3">
        <h2 class="text-lg font-semibold text-highlighted">{{ t('pages.layers.found', { count: hits, term: search.trim(), files: found.length }) }}</h2>
        <div class="grid grid-cols-[repeat(auto-fill,minmax(min(22rem,100%),1fr))] items-start gap-2.5">
          <FileCard v-for="file in found.slice(0, 150)" :key="file.path" :file="file" :label="file.path" :term="term" open />
        </div>
        <p v-if="found.length > 150" class="text-sm text-muted">{{ t('pages.layers.first150') }}</p>
      </section>
      <PartFiles v-else :part="part" :class="wide ? '' : 'rounded-lg border border-default p-4'" />
    </div>
  </div>
</template>
