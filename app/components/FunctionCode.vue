<script setup lang="ts">
import type { ThemedToken } from 'shiki/core'
import type { Source } from '~~/shared/types'

// A function's code as the file has it now, with line numbers and syntax colours; read again after every scan (a save).
const props = defineProps<{ path: string, start: number, end: number }>()
const { scan, open } = useCode()
const { repo } = useRepositories()

const source = shallowRef<Source | null>(null)
const tokens = shallowRef<ThemedToken[][] | null>(null)
const failed = ref(false)
const ask = latest(repo)

watch(() => [props.path, props.start, props.end, scan.value?.scannedAt], async () => {
  try {
    const found = await ask.run(at => haunt.source(at, props.path, props.start, props.end))
    if (found === undefined) return
    source.value = found
    failed.value = false
    tokens.value = null
    const coloured = await highlight(props.path, shown.value, { script: true })
    // The colours only while this code is still the one shown.
    if (source.value === found) tokens.value = coloured
  } catch {
    failed.value = true
  }
}, { immediate: true })

/** The indent all lines share, taken off so a nested function starts at the left. */
const shown = computed(() => {
  const lines = (source.value?.lines ?? []).map(line => line.replace(/\t/g, '  '))
  const indent = Math.min(...lines.filter(line => line.trim()).map(line => line.length - line.trimStart().length))
  return Number.isFinite(indent) && indent > 0 ? lines.map(line => line.slice(indent)) : lines
})
</script>

<template>
  <section class="grid gap-1.5">
    <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">Code</h3>
    <p v-if="failed" class="text-xs text-muted">The code could not be read.</p>
    <div v-else-if="source" class="max-h-96 overflow-auto rounded-md border border-default bg-elevated/40 py-1.5">
      <pre class="font-mono text-label leading-[1.55]"><code><span v-for="(line, n) in shown" :key="n" class="flex pr-3 hover:bg-elevated"><span class="sticky left-0 w-10 shrink-0 bg-(--ui-bg) pr-2 text-right text-dimmed select-none">{{ source.start + n }}</span><span v-if="tokens?.[n]?.length" class="pl-2 whitespace-pre"><span v-for="(token, at) in tokens[n]" :key="at" class="code-token" :style="token.htmlStyle">{{ token.content }}</span></span><span v-else class="pl-2 whitespace-pre">{{ line || ' ' }}</span></span></code></pre>
      <button v-if="source.cut" type="button" class="px-3 pt-1 text-xs text-muted hover:underline" @click="open(props.path, props.start)">The first 400 lines; open the file for the rest.</button>
    </div>
  </section>
</template>
