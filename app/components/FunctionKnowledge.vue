<script setup lang="ts">
import type { Blamed } from '~/bindings'

/*
 * Who wrote the lines a function has now, in the function panel: one `git blame` of its lines as the file is now,
 * yours alone and the ones co-written with an AI apart (WroteLines). Read again when the function, the scan or HEAD
 * changes.
 */
const props = defineProps<{ path: string, start: number, end: number }>()
const { scan } = useCode()
const { repo, gitChanged } = useRepositories()
const headChanged = gitChanged('head')

const blamed = shallowRef<Blamed | null>(null)
const ask = latest(repo)
const { loading } = ask
watch(() => [props.path, props.start, props.end, scan.value?.scannedAt, headChanged.value], async () => {
  const found = await ask.run(at => haunt.blameFunction(at, props.path, props.start, props.end)).catch(() => null)
  if (found !== undefined) blamed.value = found
}, { immediate: true })
</script>

<template>
  <section class="grid gap-1.5">
    <h3 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
      {{ t('PartKnowledge.title') }}
      <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-3 animate-spin" />
    </h3>
    <p class="text-label text-muted">{{ t('PartKnowledge.wrote') }}</p>
    <WroteLines v-if="blamed && (blamed.youAlone || blamed.ai || blamed.others.length)" :blamed="blamed" />
    <p v-else-if="blamed" class="text-xs text-muted">{{ t('PartKnowledge.noLines') }}</p>
  </section>
</template>
