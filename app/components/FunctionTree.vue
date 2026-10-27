<script setup lang="ts">
import type { FunctionNode } from '~~/shared/types'
import type { LastChange } from '~/composables/useFunctionBlame'

// Functions as boxes: a function written inside another is drawn inside it; with `lastChanges`, who changed each last.
const props = defineProps<{ items: FunctionNode[], path: string, term?: string, lastChanges?: Map<string, LastChange> | null }>()
const { index, selected, select, open } = useCode()
const { times } = usePreferences()
const age = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleDateString(locale())
  const days = Math.floor((Date.now() / 1000 - seconds) / 86_400)
  if (days < 1) return t('FunctionTree.today')
  if (days < 60) return t('FunctionTree.days', { count: days })
  if (days < 730) return t('FunctionTree.months', { count: Math.floor(days / 30) })
  return t('FunctionTree.years', { count: Math.floor(days / 365) })
}

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
        <span class="grid size-4.5 shrink-0 place-items-center rounded border border-default bg-elevated font-mono text-label" :class="KIND_COLOR[fn.kind]" :title="t(`FunctionTree.kind.${fn.kind}`)">{{ KIND[fn.kind] }}</span>
        <button
          type="button"
          class="min-w-0 flex-1 truncate text-left font-mono text-xs hover:underline"
          :class="fn.anonymous ? 'text-muted italic' : 'text-highlighted'"
          :title="t('FunctionTree.where', { name: fn.name, from: fn.line, to: fn.endLine })"
          @click="select(fn.id)"
        >
          {{ fn.name }}
        </button>
        <UBadge v-if="fn.exported" :label="t('FunctionTree.exported')" size="sm" variant="soft" color="info" class="text-label" />
        <UBadge v-if="fn.async" label="async" size="sm" variant="soft" color="secondary" class="text-label" />
        <span
          v-if="index && (calls(fn.id).in || calls(fn.id).out)"
          class="font-mono text-label whitespace-nowrap text-muted tabular-nums"
          :title="t('FunctionTree.calls', { count: calls(fn.id).in, out: calls(fn.id).out })"
        >↘{{ calls(fn.id).in }} ↗{{ calls(fn.id).out }}</span>
        <span
          v-if="props.lastChanges?.get(fn.id)"
          class="max-w-36 truncate text-label whitespace-nowrap"
          :class="props.lastChanges.get(fn.id)!.uncommitted ? 'text-warning' : 'text-muted'"
          :title="props.lastChanges.get(fn.id)!.uncommitted ? t('FunctionTree.uncommittedHelp') : t('FunctionTree.lastChange', { subject: props.lastChanges.get(fn.id)!.subject, date: new Date(props.lastChanges.get(fn.id)!.time * 1000).toLocaleString(locale()) })"
        >{{ props.lastChanges.get(fn.id)!.uncommitted ? t('FunctionTree.uncommitted') : `${props.lastChanges.get(fn.id)!.author} · ${age(props.lastChanges.get(fn.id)!.time)}` }}</span>
        <span class="font-mono text-label whitespace-nowrap text-dimmed tabular-nums">:{{ fn.line }}</span>
        <UButton icon="i-lucide-square-arrow-out-up-right" size="xs" color="neutral" variant="ghost" class="-my-1" :aria-label="t('FunctionTree.open', { name: fn.name })" @click="open(props.path, fn.line)" />
      </div>
      <FunctionTree v-if="fn.children.length" :items="fn.children" :path="props.path" :term="props.term" :last-changes="props.lastChanges" class="pl-2" />
    </div>
  </div>
</template>
