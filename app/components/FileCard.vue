<script setup lang="ts">
import type { ScannedFile } from '~~/shared/types'
import type { LastChange } from '~/composables/useFunctionBlame'

/*
 * A file with its functions, folded open or shut: how often it changed lately, its size, a bookmark (useBookmarks),
 * and, with Last changed switched on, who changed each function last and when (useFunctionBlame), read when it opens.
 */
const props = defineProps<{ file: ScannedFile, label: string, term?: string, open?: boolean, highlight?: boolean }>()
const { open: openFile } = useCode()
const count = computed(() => countFunctions(props.file.functions))
const { files: churn } = useHotspots()
const commits = computed(() => churn.value?.[props.file.path]?.commits ?? 0)
const bookmarks = useBookmarks()
const marked = computed(() => bookmarks.isBookmarked(props.file.path))

const blame = useFunctionBlame()
const shown = ref(!!props.open)
const lastChanges = shallowRef<Map<string, LastChange> | null>(null)
watch([shown, blame.on, () => props.file], async ([open, on]) => {
  if (!open || !on) return
  const file = props.file
  const found = await blame.of(file.path, file.functions)
  if (file === props.file) lastChanges.value = found
}, { immediate: true })

const card = useTemplateRef<HTMLElement>('card')
onMounted(() => {
  if (props.highlight) card.value?.scrollIntoView({ block: 'center' })
})
</script>

<template>
  <details ref="card" class="group min-w-0 rounded-md border border-default bg-elevated/40" :class="props.highlight ? 'ring-2 ring-primary' : ''" :open="props.open" @toggle="shown = ($event.target as HTMLDetailsElement).open">
    <summary class="flex cursor-pointer list-none items-baseline justify-between gap-2 px-2.5 py-2 [&::-webkit-details-marker]:hidden">
      <span class="flex min-w-0 items-center gap-1.5">
        <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-muted transition group-open:rotate-90" />
        <span class="font-mono text-xs font-medium break-all">{{ props.label }}</span>
        <UBadge v-if="props.file.test" :label="t('FileCard.test')" size="sm" variant="outline" color="neutral" class="text-label" />
      </span>
      <span class="flex items-center gap-1 text-label whitespace-nowrap text-muted tabular-nums">
        <span v-if="commits" class="rounded bg-(--rule-known)/15 px-1 text-(--rule-known)" :title="t('FileCard.commits', { count: commits })">{{ commits }}×</span>
        {{ t('FileCard.functions', { count }) }} · {{ t('FileCard.lines', { count: props.file.lines }) }}
        <UButton
          :icon="marked ? 'i-lucide-bookmark-check' : 'i-lucide-bookmark'"
          size="xs"
          :color="marked ? 'primary' : 'neutral'"
          variant="ghost"
          class="-my-1"
          :class="marked ? '' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'"
          :aria-label="marked ? t('FileCard.unbookmark') : t('FileCard.bookmark')"
          :title="marked ? t('FileCard.unbookmark') : t('FileCard.bookmark')"
          @click.prevent="bookmarks.toggle(props.file.path)"
        />
        <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" class="-my-1" :aria-label="t('FileCard.open')" @click.prevent="openFile(props.file.path)" />
      </span>
    </summary>
    <div class="border-t border-default p-2">
      <FunctionTree v-if="props.file.functions.length" :items="props.file.functions" :path="props.file.path" :term="props.term" :last-changes="blame.on.value ? lastChanges : null" />
      <p v-else class="px-1 text-xs text-muted">{{ t('FileCard.none') }}</p>
    </div>
  </details>
</template>
