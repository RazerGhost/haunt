<script setup lang="ts">
import type { ThemedToken } from 'shiki/core'

// A few lines of code with syntax colours for their file's language (plain while the colours load, or for a language
// Shiki does not have here); for the conflict view's blocks.
const props = defineProps<{ path: string, text: string }>()
const lines = computed(() => props.text.replace(/\r?\n$/, '').split(/\r?\n/))
const tokens = shallowRef<ThemedToken[][] | null>(null)
watch(() => [props.path, props.text], async () => {
  tokens.value = null
  const wanted = lines.value
  const found = await highlight(props.path, wanted)
  if (wanted === lines.value) tokens.value = found
}, { immediate: true })
</script>

<template>
  <pre class="overflow-x-auto font-mono text-xs whitespace-pre"><template v-for="(line, at) in lines" :key="at"><span v-if="tokens?.[at]?.length"><span v-for="(token, index) in tokens[at]" :key="index" class="code-token" :style="token.htmlStyle as Record<string, string>">{{ token.content }}</span></span><span v-else>{{ line }}</span>{{ at < lines.length - 1 ? '\n' : '' }}</template></pre>
</template>
