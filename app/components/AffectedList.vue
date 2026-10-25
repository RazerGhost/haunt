<script setup lang="ts">
import type { Affected } from '~/utils/impact'

// The functions that call a changed one, nearest first, each with the changed function it reaches.
const props = defineProps<{ affected: Map<string, Affected> }>()
const { index, select, open } = useCode()

const nearest = computed(() => [...props.affected]
  .map(([id, item]) => ({ id, ...item, located: index.value?.functions.get(id), via: index.value?.functions.get(item.via)?.fn.name ?? '' }))
  .filter(item => item.located)
  .sort((a, b) => a.distance - b.distance || a.located!.fn.name.localeCompare(b.located!.fn.name)))
</script>

<template>
  <p v-if="!nearest.length" class="text-sm text-muted">No function calls the changed ones{{ index?.callers.size ? '' : ' (the call graph is still being worked out)' }}.</p>
  <ul v-else class="grid gap-1">
    <li v-for="item in nearest" :key="item.id" class="flex min-w-0 items-center gap-2 rounded px-2 py-1 hover:bg-elevated">
      <span class="grid size-5 shrink-0 place-items-center rounded-full border border-default font-mono text-label text-muted" :title="`${item.distance} step${item.distance > 1 ? 's' : ''} from a change`">{{ item.distance }}</span>
      <button type="button" class="grid min-w-0 flex-1 text-left" @click="select(item.id)">
        <span class="truncate font-mono text-xs text-highlighted hover:underline">{{ item.located!.fn.name }}</span>
        <span class="truncate font-mono text-label text-muted">{{ tailOf(item.located!.file.path) }} · reaches {{ item.via }}</span>
      </button>
      <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" :aria-label="`Open ${item.located!.fn.name} in the editor`" @click="open(item.located!.file.path, item.located!.fn.line)" />
    </li>
  </ul>
</template>
