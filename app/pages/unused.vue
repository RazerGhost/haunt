<script setup lang="ts">
import type { CommitInfo } from '~/bindings'
import { unusedFunctions, type Unused, type UnusedKind } from '~~/shared/unused'
import { addTo } from '~~/shared/collect'

/*
 * What looks unused (shared/unused.ts), by part: functions nothing uses (likely dead), exports nothing in the scan uses
 * (maybe used from outside: a tool, another program), and exports used only in their own file (the export can go). A use is any reference the type checker sees (a call, a function passed
 * or stored, a Nuxt auto-import, a framework template's mention), so what is listed is worth a look, not a verdict: a
 * function may be used in ways no scan sees (by name, from another program). Functions written into objects or given as
 * values where they are written (`{ onSelect: () => … }`, a returned function) count as used. "When?" finds the last commit in which
 * another file gained or lost a mention of the name: about when its last use went.
 */
const { scan, index, uses, select, open } = useCode()
const { repo } = useRepositories()
const { times } = usePreferences()

const KINDS: { kind: UnusedKind, label: string, what: string }[] = [
  { kind: 'dead', label: 'Nothing uses them', what: 'Not exported, and no call, reference or template mention the scan can see: likely dead.' },
  { kind: 'unreferenced', label: 'Exported, unused here', what: 'Exported, but nothing in the scan uses them: dead, or used from outside (a tool or script).' },
  { kind: 'export', label: 'Used only in their file', what: 'Exported, but used only in the file that exports them: the export can go.' }
]
const kind = ref<UnusedKind>('dead')
const all = computed(() => (scan.value && uses.value ? unusedFunctions(scan.value.files, uses.value) : null))
const shown = computed(() => (all.value ?? []).filter(item => item.kind === kind.value))
const byPart = computed(() => {
  const parts = new Map<string, Unused[]>()
  for (const item of shown.value) addTo(parts, item.part, item)
  return [...parts]
})
const count = (wanted: UnusedKind) => all.value?.filter(item => item.kind === wanted).length ?? 0

const when = reactive(new Map<string, CommitInfo | null | 'asking' | 'failed'>())
async function ask(item: Unused): Promise<void> {
  if (!repo.value) return
  when.set(item.id, 'asking')
  try {
    when.set(item.id, await haunt.lastMention(repo.value, item.name, item.path))
  } catch {
    when.set(item.id, 'failed')
  }
}
const date = (seconds: number) => times.value === 'date' ? new Date(seconds * 1000).toLocaleString() : new Date(seconds * 1000).toLocaleDateString()
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex flex-wrap items-center gap-3 border-b border-default px-4 py-2">
      <UFieldGroup size="sm">
        <UButton v-for="item in KINDS" :key="item.kind" :label="`${item.label} · ${count(item.kind)}`" :color="kind === item.kind ? 'primary' : 'neutral'" :variant="kind === item.kind ? 'solid' : 'outline'" @click="kind = item.kind" />
      </UFieldGroup>
      <p class="min-w-0 flex-1 text-xs text-muted">
        {{ KINDS.find(item => item.kind === kind)?.what }}
        A hint, not a verdict. Left out: tests, methods, default exports, framework-loaded files and entry exports.
      </p>
    </div>
    <p v-if="!scan" class="p-6 text-center text-sm text-muted">Waiting for the scan.</p>
    <p v-else-if="!uses" class="flex items-center justify-center gap-2 p-6 text-sm text-muted"><UIcon name="i-lucide-loader-circle" class="size-4 animate-spin" />Working out the calls and uses…</p>
    <p v-else-if="!shown.length" class="p-6 text-center text-sm text-muted">{{ kind === 'export' ? 'No export is used only in its own file.' : kind === 'dead' ? 'Every function that is not exported is used.' : 'Every export is used somewhere in the scan.' }}</p>
    <div v-else class="grid min-h-0 flex-1 content-start gap-4 overflow-auto p-4">
      <section v-for="[part, items] in byPart" :key="part" class="grid gap-1">
        <h2 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
          <span class="size-2 rounded-full" :style="{ background: layerColor(index?.parts.get(part)?.layer ?? '') }" />{{ part }} <span class="font-normal tabular-nums">{{ items.length }}</span>
        </h2>
        <ul class="grid gap-0.5">
          <li v-for="item in items" :key="item.id" class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 rounded px-2 py-1 hover:bg-elevated">
            <button type="button" class="min-w-0 truncate font-mono text-xs text-highlighted hover:underline" :title="`Show ${item.name} in the panel`" @click="select(item.id)">{{ item.name }}</button>
            <button type="button" class="min-w-0 truncate font-mono text-label text-muted hover:underline" :title="`Open ${item.path}:${item.line} in the editor`" @click="open(item.path, item.line)">{{ item.path }}:{{ item.line }}</button>
            <span class="ml-auto flex items-center gap-1.5 text-label text-muted">
              <template v-if="when.get(item.id) === 'asking'"><UIcon name="i-lucide-loader-circle" class="size-3 animate-spin" />Looking…</template>
              <template v-else-if="when.get(item.id) === 'failed'">Could not tell.</template>
              <template v-else-if="when.get(item.id) === null">No other file ever mentioned it.</template>
              <NuxtLink v-else-if="when.get(item.id)" :to="{ path: '/commits', query: { commit: (when.get(item.id) as CommitInfo).sha } }" class="truncate hover:underline" :title="`The last commit where another file mentioned ${item.name}`">
                Last mentioned elsewhere: {{ (when.get(item.id) as CommitInfo).subject }} · {{ date((when.get(item.id) as CommitInfo).time) }}
              </NuxtLink>
              <UButton v-else label="When?" size="xs" color="neutral" variant="ghost" title="Roughly when its last use went: the last commit where another file mentioned it" @click="ask(item)" />
            </span>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>
