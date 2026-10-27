<script setup lang="ts">
import type { Box } from '~~/shared/canvas'
import { canvasSvg } from '~~/shared/exportSvg'

/*
 * The canvas: the code as boxes in boxes (CodeCanvas), with hotspots as a tint and the search marking the functions
 * it finds. Layers show what sits on what; the canvas shows what sits in what.
 */
const { scan, status, message, search } = useCode()
const { on: hotspots, files: churn, days } = useHotspots()
const { repo } = useRepositories()
const toast = useToast()
const failure = useFailure()
const board = useTemplateRef<{ fit: () => void, tree: { root: Box } | null }>('board')

/* Export: the whole canvas, with the files it shows, in the theme's colours. */
const exporting = ref(false)
async function exportAs(format: ImageFormat): Promise<void> {
  const root = board.value?.tree?.root
  if (!root || exporting.value) return
  exporting.value = true
  try {
    const svg = canvasSvg(root, { width: 3200, colors: themeColors() })
    const name = `${repo.value ? folderName(repo.value) : 'haunt'}-canvas.${format}`
    const path = await saveImage(svg, format, name, t('pages.canvas.exportTitle'), t(`pages.canvas.${format}`))
    if (path) toast.add({ title: t('pages.canvas.exported'), description: path, icon: 'i-lucide-image', color: 'success' })
  } catch (error) {
    failure.failed(t('pages.canvas.notExported'), error)
  } finally {
    exporting.value = false
  }
}
const exportItems = computed(() => [[
  { label: t('pages.canvas.png'), icon: 'i-lucide-image', onSelect: () => { void exportAs('png') } },
  { label: t('pages.canvas.svg'), icon: 'i-lucide-file-code', onSelect: () => { void exportAs('svg') } }
]])

/** Per file, a 0–1 score of how often it changed, on a log scale against the busiest file. */
const heat = computed(() => {
  if (!churn.value) return null
  const counts = Object.entries(churn.value).map(([path, item]) => [path, item?.commits ?? 0] as const)
  const top = Math.log1p(Math.max(0, ...counts.map(([, commits]) => commits)))
  return new Map(counts.map(([path, commits]) => [path, top ? Math.log1p(commits) / top : 0]))
})
</script>

<template>
  <div v-if="!scan" class="grid justify-items-center gap-2 py-16 text-muted">
    <UIcon v-if="status !== 'error'" name="i-lucide-loader-circle" class="size-5 animate-spin" />
    <p>{{ message || t('pages.canvas.scanning') }}</p>
  </div>
  <div v-else class="flex h-full flex-col">
    <div class="flex items-center gap-3 border-b border-default px-3 py-1.5 text-xs text-muted">
      <span class="truncate">{{ t('pages.canvas.hint') }}</span>
      <div class="ml-auto flex shrink-0 items-center gap-3">
        <USwitch v-model="hotspots" :label="t('pages.canvas.hotspots')" size="xs" :title="t('pages.canvas.hotspotsHelp', { count: days })" />
        <UButton icon="i-lucide-maximize" :label="t('pages.canvas.fit')" size="xs" color="neutral" variant="ghost" @click="board?.fit()" />
        <UDropdownMenu :items="exportItems" :content="{ align: 'end' }">
          <UButton icon="i-lucide-download" :label="t('pages.canvas.export')" size="xs" color="neutral" variant="ghost" :loading="exporting" />
        </UDropdownMenu>
      </div>
    </div>
    <div class="px-3 empty:hidden [&:has(*)]:pt-2">
      <TrustLayersNote />
    </div>
    <div class="min-h-0 flex-1">
      <CodeCanvas ref="board" :heat="heat" :term="search" />
    </div>
  </div>
</template>
