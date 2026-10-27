<script setup lang="ts">
import type { ScannedFile } from '~~/shared/types'

const props = defineProps<{ file: ScannedFile, label: string, term?: string, open?: boolean }>()
const { open: openFile } = useCode()
const count = computed(() => countFunctions(props.file.functions))
const { files: churn } = useHotspots()
const commits = computed(() => churn.value?.[props.file.path]?.commits ?? 0)
</script>

<template>
  <details class="group min-w-0 rounded-md border border-default bg-elevated/40" :open="props.open">
    <summary class="flex cursor-pointer list-none items-baseline justify-between gap-2 px-2.5 py-2 [&::-webkit-details-marker]:hidden">
      <span class="flex min-w-0 items-center gap-1.5">
        <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-muted transition group-open:rotate-90" />
        <span class="font-mono text-xs font-medium break-all">{{ props.label }}</span>
        <UBadge v-if="props.file.test" :label="t('FileCard.test')" size="sm" variant="outline" color="neutral" class="text-label" />
      </span>
      <span class="flex items-center gap-1 text-label whitespace-nowrap text-muted tabular-nums">
        <span v-if="commits" class="rounded bg-(--rule-known)/15 px-1 text-(--rule-known)" :title="t('FileCard.commits', { count: commits })">{{ commits }}×</span>
        {{ t('FileCard.functions', { count }) }} · {{ t('FileCard.lines', { count: props.file.lines }) }}
        <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" class="-my-1" :aria-label="t('FileCard.open')" @click.prevent="openFile(props.file.path)" />
      </span>
    </summary>
    <div class="border-t border-default p-2">
      <FunctionTree v-if="props.file.functions.length" :items="props.file.functions" :path="props.file.path" :term="props.term" />
      <p v-else class="px-1 text-xs text-muted">{{ t('FileCard.none') }}</p>
    </div>
  </details>
</template>
