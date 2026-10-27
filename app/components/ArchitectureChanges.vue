<script setup lang="ts">
import type { Import, ImportChanges } from '~/bindings'
import { importGraph, reaches } from '~~/shared/reach'

/*
 * What a change does to the architecture (src-tauri/src/code/architecture.rs): the imports it adds and removes, by
 * file, each judged by the project's rules (a layer broken, a part entered past its door), and the added run-time ones
 * that would close a cycle in the imports as they are now. Read for the commit, or the two commits, picked.
 */
const props = defineProps<{ from: string | null, to: string }>()
const { repo } = useRepositories()
const { scan, open } = useCode()

const changes = shallowRef<ImportChanges | null>(null)
const problem = ref('')
const ask = latest(repo)
watch(() => [props.from, props.to, repo.value], async () => {
  changes.value = null
  problem.value = ''
  try {
    const found = await ask.run(at => haunt.changeImports(at, props.from, props.to))
    if (found !== undefined) changes.value = found
  } catch (error) {
    problem.value = errorMessage(error, t('ArchitectureChanges.readError'))
  }
}, { immediate: true })

/** The run-time imports there are now, between and within parts. */
const graph = computed(() => importGraph([
  ...(scan.value?.imports ?? []).flatMap(group => group.files),
  ...(scan.value?.innerImports ?? [])
]))
const closesCycle = (from: string, edge: Import) => !edge.typeOnly && reaches(graph.value, edge.to, from)

const totals = computed(() => {
  const files = changes.value?.files ?? []
  const added = files.flatMap(file => file.added)
  const removed = files.flatMap(file => file.removed)
  const between = (edges: Import[]) => edges.filter(edge => edge.fromPart !== edge.toPart).length
  return {
    added: added.length,
    removed: removed.length,
    addedBetween: between(added),
    removedBetween: between(removed),
    addedBreaks: added.filter(edge => edge.breaks.length).length,
    removedBreaks: removed.filter(edge => edge.breaks.length).length,
    cycles: files.reduce((sum, file) => sum + file.added.filter(edge => closesCycle(file.path, edge)).length, 0)
  }
})
const RULES = ['layer', 'door']
/** A rule an import breaks (or, `removed`, no longer breaks), in words; a rule without words shows its name. */
function ruleLabel(rule: string, removed = false): string {
  if (RULES.includes(rule)) return t(`ArchitectureChanges.${removed ? 'ruleRemoved' : 'rule'}.${rule}`)
  return removed ? t('ArchitectureChanges.noLonger', { rule }) : rule
}
</script>

<template>
  <div class="grid content-start gap-4">
    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <p v-else-if="!changes" class="flex items-center gap-2 text-sm text-muted"><UIcon name="i-lucide-loader-circle" class="size-4 animate-spin" />{{ t('ArchitectureChanges.reading') }}</p>
    <template v-else>
      <div class="grid gap-1 text-sm">
        <p v-if="!changes.files.length" class="text-muted">{{ t('ArchitectureChanges.noImports') }}</p>
        <template v-else>
          <p v-if="changes.judged" class="flex flex-wrap items-center gap-x-3 gap-y-1">
            <span :class="totals.addedBreaks ? 'font-medium text-error' : 'text-muted'">{{ t('ArchitectureChanges.addsBreaks', { count: totals.addedBreaks }) }}</span>
            <span :class="totals.removedBreaks ? 'font-medium text-success' : 'text-muted'">· {{ t('ArchitectureChanges.removesBreaks', { count: totals.removedBreaks }) }}</span>
            <span v-if="totals.cycles" class="font-medium text-error">· {{ t('ArchitectureChanges.cycles', { count: totals.cycles }) }}</span>
          </p>
          <p class="text-xs text-muted">
            {{ t('ArchitectureChanges.totals', { count: totals.added, addedBetween: totals.addedBetween, removed: totals.removed, removedBetween: totals.removedBetween }) }}
            <template v-if="!changes.judged">{{ t('ArchitectureChanges.noRules') }}</template>
            {{ t('ArchitectureChanges.judgedNow') }}
          </p>
        </template>
      </div>
      <section v-for="file in changes.files" :key="file.path" class="rounded-md border border-default">
        <button type="button" class="block w-full truncate border-b border-default px-3 py-1.5 text-left font-mono text-xs font-medium hover:underline" :title="t('ArchitectureChanges.openFile', { path: file.path })" @click="open(file.path)">{{ file.path }}</button>
        <ul class="grid gap-0.5 p-1.5">
          <li v-for="edge in file.added" :key="`+${edge.to}`" class="flex min-w-0 flex-wrap items-center gap-x-2 rounded px-1.5 py-0.5 text-xs">
            <span class="w-3 shrink-0 font-mono text-success">+</span>
            <span class="min-w-0 truncate font-mono" :title="edge.to">{{ edge.to }}</span>
            <span v-if="edge.fromPart !== edge.toPart" class="font-mono text-label text-muted">{{ edge.fromPart }} → {{ edge.toPart }}</span>
            <span v-if="edge.typeOnly" class="text-label text-dimmed">{{ t('ArchitectureChanges.typesOnly') }}</span>
            <UBadge v-for="rule in edge.breaks" :key="rule" :label="ruleLabel(rule)" color="error" variant="subtle" size="sm" />
            <UBadge v-if="closesCycle(file.path, edge)" :label="t('ArchitectureChanges.closesCycle')" color="error" variant="outline" size="sm" />
          </li>
          <li v-for="edge in file.removed" :key="`-${edge.to}`" class="flex min-w-0 flex-wrap items-center gap-x-2 rounded px-1.5 py-0.5 text-xs text-muted">
            <span class="w-3 shrink-0 font-mono text-error">−</span>
            <span class="min-w-0 truncate font-mono line-through" :title="edge.to">{{ edge.to }}</span>
            <span v-if="edge.fromPart !== edge.toPart" class="font-mono text-label">{{ edge.fromPart }} → {{ edge.toPart }}</span>
            <UBadge v-for="rule in edge.breaks" :key="rule" :label="ruleLabel(rule, true)" color="success" variant="subtle" size="sm" />
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
