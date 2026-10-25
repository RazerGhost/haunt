<script setup lang="ts">
import type { FunctionNode } from '~~/shared/types'

// Functions as boxes: a function written inside another is drawn inside it.
const props = defineProps<{ items: FunctionNode[], path: string, term?: string }>()
const { index, selected, select, open } = useCode()

const KIND = { function: 'f', arrow: 'a', method: 'm', class: 'c' } as const
const KIND_COLOR = {
  function: 'text-sky-600 dark:text-sky-400',
  arrow: 'text-amber-600 dark:text-amber-400',
  method: 'text-emerald-600 dark:text-emerald-400',
  class: 'text-rose-600 dark:text-rose-400'
} as const

function calls(id: string) {
  return { out: index.value?.callees.get(id)?.length ?? 0, in: index.value?.callers.get(id)?.length ?? 0 }
}
</script>

<template>
  <div class="grid gap-1">
    <div
      v-for="fn in props.items"
      :key="fn.id"
      class="grid min-w-0 gap-1 rounded border border-l-[3px] border-default py-1 pr-1.5 pl-2"
      :class="[
        fn.anonymous ? 'border-dashed bg-transparent' : 'bg-default',
        selected?.fn.id === fn.id ? 'ring-2 ring-inverted' : '',
        props.term && fn.name.toLowerCase().includes(props.term) ? 'ring-2 ring-amber-500' : ''
      ]"
    >
      <div class="flex min-w-0 items-center gap-1.5">
        <span class="grid size-4.5 shrink-0 place-items-center rounded border border-default bg-elevated font-mono text-label" :class="KIND_COLOR[fn.kind]" :title="fn.kind">{{ KIND[fn.kind] }}</span>
        <button
          type="button"
          class="min-w-0 flex-1 truncate text-left font-mono text-xs hover:underline"
          :class="fn.anonymous ? 'text-muted italic' : 'text-highlighted'"
          :title="`${fn.name} (lines ${fn.line}–${fn.endLine})`"
          @click="select(fn.id)"
        >
          {{ fn.name }}
        </button>
        <UBadge v-if="fn.exported" label="exp" size="sm" variant="soft" color="info" class="text-label" />
        <UBadge v-if="fn.async" label="async" size="sm" variant="soft" color="secondary" class="text-label" />
        <span
          v-if="index && (calls(fn.id).in || calls(fn.id).out)"
          class="font-mono text-label whitespace-nowrap text-muted tabular-nums"
          :title="`Called from ${calls(fn.id).in} functions, calls ${calls(fn.id).out}`"
        >↘{{ calls(fn.id).in }} ↗{{ calls(fn.id).out }}</span>
        <span class="font-mono text-label whitespace-nowrap text-dimmed tabular-nums">:{{ fn.line }}</span>
        <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" class="-my-1" :aria-label="`Open ${fn.name} in the editor`" @click="open(props.path, fn.line)" />
      </div>
      <FunctionTree v-if="fn.children.length" :items="fn.children" :path="props.path" :term="props.term" class="pl-2" />
    </div>
  </div>
</template>
