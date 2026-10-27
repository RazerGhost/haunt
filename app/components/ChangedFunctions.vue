<script setup lang="ts">
import type { ChangedFile, ChangedFunction } from '~~/shared/types'
import { addTo } from '~~/shared/collect'

/*
 * Changed files by part, each with its functions added, changed or removed. A function the code still has can be
 * selected and opened; a removed one is struck through. Used by the local changes and the commit list's lenses.
 */
const props = defineProps<{ files: ChangedFile[] }>()
const emit = defineEmits<{ file: [file: ChangedFile] }>()
const { index, select, open } = useCode()

const byPart = computed(() => {
  const parts = new Map<string, ChangedFile[]>()
  for (const file of props.files) {
    const part = file.part ?? t('ChangedFunctions.noPart')
    addTo(parts, part, file)
  }
  return [...parts].sort(([a], [b]) => a.localeCompare(b))
})

// Each status's label is `ChangedFunctions.status.<status>`.
const STATUS: Record<ChangedFunction['status'], { color: 'success' | 'warning' | 'error' | 'info' }> = {
  added: { color: 'success' },
  changed: { color: 'warning' },
  removed: { color: 'error' },
  moved: { color: 'info' }
}
/** The files a function moved out of, to say so where the file lists no function of its own. */
const movedOut = computed(() => new Set(props.files.flatMap(file => file.functions.flatMap(fn => (fn.from ? [fn.from.path] : [])))))
/** Where a moved function came from, in words: another name, another file, or both. */
function movedFrom(fn: ChangedFunction, path: string): string {
  const from = fn.from!
  const renamed = from.name !== fn.name
  if (from.path === path) return renamed ? t('ChangedFunctions.renamedFrom', { name: from.name }) : t('ChangedFunctions.movedFrom', { name: from.qualified })
  return renamed ? t('ChangedFunctions.fromNameIn', { name: from.name, file: tailOf(from.path) }) : t('ChangedFunctions.fromFile', { file: tailOf(from.path) })
}
</script>

<template>
  <div class="grid gap-5">
    <div v-for="[part, files] in byPart" :key="part" class="grid gap-2">
      <h2 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
        <span class="size-2 rounded-full" :style="{ background: layerColor(index?.parts.get(part)?.layer ?? '') }" />{{ part }}
      </h2>
      <div v-for="file in files" :key="file.path" class="rounded-md border border-default bg-elevated/40">
        <div class="flex items-center justify-between gap-2 border-b border-default px-3 py-2">
          <button type="button" class="flex min-w-0 items-center gap-2 text-left" @click="emit('file', file)">
            <UBadge :label="t(`ChangedFunctions.fileStatus.${file.status}`)" size="sm" variant="outline" color="neutral" />
            <span class="truncate font-mono text-xs font-medium hover:underline" :title="file.status === 'renamed' ? `${file.oldPath} → ${file.path}` : file.path">{{ file.path }}</span>
          </button>
          <span class="flex items-center gap-1 font-mono text-label whitespace-nowrap tabular-nums">
            <span class="text-success">+{{ file.added }}</span><span class="text-error">−{{ file.removed }}</span>
            <UButton v-if="file.part" icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" :aria-label="t('ChangedFunctions.openFile')" @click="open(file.path)" />
          </span>
        </div>
        <ul v-if="file.functions.length" class="grid gap-1 p-2">
          <li v-for="fn in file.functions" :key="fn.qualified" class="flex min-w-0 items-center gap-2 rounded px-2 py-1 hover:bg-elevated">
            <UBadge :label="t(`ChangedFunctions.status.${fn.status}`)" :color="STATUS[fn.status].color" size="sm" variant="soft" class="w-16 justify-center" />
            <button v-if="fn.id" type="button" class="min-w-0 flex-1 truncate text-left font-mono text-xs text-highlighted hover:underline" :title="fn.qualified" @click="select(fn.id)">{{ fn.qualified }}</button>
            <span v-else class="min-w-0 flex-1 truncate font-mono text-xs text-muted" :class="{ 'line-through': fn.status === 'removed' }" :title="fn.status === 'removed' ? fn.qualified : t('ChangedFunctions.notInCode', { name: fn.qualified })">{{ fn.qualified }}</span>
            <span v-if="fn.from" class="min-w-0 shrink truncate font-mono text-label text-info" :title="`${fn.from.path}:${fn.from.line} (${fn.from.qualified})`">{{ movedFrom(fn, file.path) }}</span>
            <span class="font-mono text-label whitespace-nowrap tabular-nums"><span class="text-success">+{{ fn.added }}</span> <span class="text-error">−{{ fn.removed }}</span></span>
            <span class="font-mono text-label text-dimmed tabular-nums">:{{ fn.line }}</span>
            <UButton v-if="fn.id" icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" :aria-label="t('ChangedFunctions.openFunction', { name: fn.name })" @click="open(file.path, index?.functions.get(fn.id)?.fn.line ?? fn.line)" />
          </li>
        </ul>
        <p v-else-if="file.unread" class="px-3 py-2 text-xs text-warning">{{ t('ChangedFunctions.unread') }}</p>
        <p v-else-if="movedOut.has(file.oldPath)" class="px-3 py-2 text-xs text-muted">{{ t('ChangedFunctions.movedOut') }}</p>
        <p v-else class="px-3 py-2 text-xs text-muted">{{ t('ChangedFunctions.noFunction') }}</p>
      </div>
    </div>
  </div>
</template>
