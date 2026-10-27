<script setup lang="ts">
/*
 * The canvas: the code as boxes in boxes (CodeCanvas), with hotspots as a tint and the search marking the functions
 * it finds. Layers show what sits on what; the canvas shows what sits in what.
 */
const { scan, status, message, search } = useCode()
const { on: hotspots, files: churn, days } = useHotspots()
const board = useTemplateRef<{ fit: () => void }>('board')

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
