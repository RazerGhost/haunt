<script setup lang="ts">
// A function in a list: its name and where it is; a click selects it.
const props = defineProps<{ id: string, count?: number, guessed?: boolean }>()
const { index, select, open } = useCode()
const found = computed(() => index.value?.functions.get(props.id) ?? null)
</script>

<template>
  <li v-if="found" class="flex min-w-0 items-center gap-1.5" :style="{ '--c': layerColor(index?.parts.get(found.file.part)?.layer ?? '') }">
    <span class="size-2 shrink-0 rounded-full bg-(--c)" :title="found.file.part" />
    <button type="button" class="grid min-w-0 flex-1 text-left" @click="select(props.id)">
      <span class="truncate font-mono text-xs text-highlighted hover:underline">{{ found.fn.name }}</span>
      <span class="truncate font-mono text-label text-muted">{{ tailOf(found.file.path) }}:{{ found.fn.line }}</span>
    </button>
    <span v-if="props.guessed" class="text-label text-muted" title="Matched by name, without a type checker: likely, not certain">≈</span>
    <span v-if="props.count && props.count > 1" class="text-label text-muted tabular-nums" :title="`${props.count} call sites`">×{{ props.count }}</span>
    <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" :aria-label="`Open ${found.fn.name} in the editor`" @click="open(found.file.path, found.fn.line)" />
  </li>
</template>
