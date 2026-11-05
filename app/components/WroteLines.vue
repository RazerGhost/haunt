<script setup lang="ts">
import type { Blamed } from '~/bindings'

/*
 * Who wrote the lines some code has now, split three ways (git/knowledge.rs): yours alone, the ones an AI co-wrote
 * (their commit names one in a Co-Authored-By trailer; which co-authors count is haunt.config.json's
 * `attribution.ai`), and everyone else's. A bar and the shares under it; the part page's "Who knows it" and the
 * function panel's both show it.
 */
const props = defineProps<{ blamed: Blamed }>()

interface Slice { key: string, label: string, lines: number, color: string }

const slices = computed<Slice[]>(() => {
  const found = props.blamed
  const out: Slice[] = []
  if (found.youAlone) out.push({ key: 'you', label: t('WroteLines.youAlone'), lines: found.youAlone, color: 'var(--c, var(--ui-primary))' })
  if (found.ai) out.push({ key: 'ai', label: t('WroteLines.ai'), lines: found.ai, color: 'var(--ui-secondary)' })
  found.others.forEach((author, n) => out.push({
    key: `other:${author.name}`,
    label: author.name,
    lines: author.lines,
    color: `color-mix(in srgb, var(--ui-text-muted) ${Math.max(20, 80 - n * 15)}%, var(--ui-bg-elevated))`
  }))
  return out
})
const total = computed(() => slices.value.reduce((sum, slice) => sum + slice.lines, 0))
const share = (lines: number) => Math.round(lines / total.value * 100)
</script>

<template>
  <div v-if="total" class="grid gap-1">
    <div class="flex h-1.5 overflow-hidden rounded-full bg-elevated">
      <i v-for="slice in slices.slice(0, 8)" :key="slice.key" class="block h-full" :style="{ width: `${slice.lines / total * 100}%`, background: slice.color }" :title="`${slice.label}: ${t('WroteLines.lines', { count: slice.lines })}`" />
    </div>
    <p class="flex flex-wrap gap-x-2.5 gap-y-0.5 text-xs">
      <span v-for="slice in slices.slice(0, 5)" :key="slice.key" class="flex items-center gap-1" :title="slice.key === 'you' && blamed.you ? blamed.you : undefined">
        <i class="size-2 rounded-full" :style="{ background: slice.color }" />{{ slice.label }} <span class="text-muted tabular-nums">{{ share(slice.lines) }}%</span>
      </span>
    </p>
  </div>
</template>
