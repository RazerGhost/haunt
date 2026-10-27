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

const KINDS: UnusedKind[] = ['dead', 'unreferenced', 'export']
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
        <UButton v-for="item in KINDS" :key="item" :label="`${t(`pages.unused.kind.${item}.label`)} · ${count(item)}`" :color="kind === item ? 'primary' : 'neutral'" :variant="kind === item ? 'solid' : 'outline'" @click="kind = item" />
      </UFieldGroup>
      <p class="min-w-0 flex-1 text-xs text-muted">
        {{ t(`pages.unused.kind.${kind}.what`) }}
        {{ t('pages.unused.hint') }}
      </p>
    </div>
    <p v-if="!scan" class="p-6 text-center text-sm text-muted">{{ t('pages.unused.waiting') }}</p>
    <p v-else-if="!uses" class="flex items-center justify-center gap-2 p-6 text-sm text-muted"><UIcon name="i-lucide-loader-circle" class="size-4 animate-spin" />{{ t('pages.unused.working') }}</p>
    <p v-else-if="!shown.length" class="p-6 text-center text-sm text-muted">{{ t(`pages.unused.kind.${kind}.empty`) }}</p>
    <div v-else class="grid min-h-0 flex-1 content-start gap-4 overflow-auto p-4">
      <section v-for="[part, items] in byPart" :key="part" class="grid gap-1">
        <h2 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
          <span class="size-2 rounded-full" :style="{ background: layerColor(index?.parts.get(part)?.layer ?? '') }" />{{ part }} <span class="font-normal tabular-nums">{{ items.length }}</span>
        </h2>
        <ul class="grid gap-0.5">
          <li v-for="item in items" :key="item.id" class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 rounded px-2 py-1 hover:bg-elevated">
            <button type="button" class="min-w-0 truncate font-mono text-xs text-highlighted hover:underline" :title="t('pages.unused.show', { name: item.name })" @click="select(item.id)">{{ item.name }}</button>
            <button type="button" class="min-w-0 truncate font-mono text-label text-muted hover:underline" :title="t('pages.unused.open', { place: `${item.path}:${item.line}` })" @click="open(item.path, item.line)">{{ item.path }}:{{ item.line }}</button>
            <span class="ml-auto flex items-center gap-1.5 text-label text-muted">
              <template v-if="when.get(item.id) === 'asking'"><UIcon name="i-lucide-loader-circle" class="size-3 animate-spin" />{{ t('pages.unused.looking') }}</template>
              <template v-else-if="when.get(item.id) === 'failed'">{{ t('pages.unused.failed') }}</template>
              <template v-else-if="when.get(item.id) === null">{{ t('pages.unused.never') }}</template>
              <NuxtLink v-else-if="when.get(item.id)" :to="{ path: '/commits', query: { commit: (when.get(item.id) as CommitInfo).sha } }" class="truncate hover:underline" :title="t('pages.unused.lastTitle', { name: item.name })">
                {{ t('pages.unused.last', { subject: (when.get(item.id) as CommitInfo).subject, date: date((when.get(item.id) as CommitInfo).time) }) }}
              </NuxtLink>
              <UButton v-else :label="t('pages.unused.when')" size="xs" color="neutral" variant="ghost" :title="t('pages.unused.whenHelp')" @click="ask(item)" />
            </span>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>
