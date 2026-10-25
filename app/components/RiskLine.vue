<script setup lang="ts">
import type { ChangedFile } from '~~/shared/types'
import { busyFiles, riskOf } from '~~/shared/risk'
import { testsReaching } from '~~/shared/testReach'

/*
 * A change's risk in one line (shared/risk.ts): low, medium or high, and why: the functions that call what changed
 * (two steps out), the changed functions no test reaches, the busy files it touches, its size. Measured on the code as
 * it is now, like the lenses.
 */
const props = defineProps<{ files: ChangedFile[] }>()
const { index } = useCode()
const hotspots = useHotspots()
hotspots.want()

const risk = computed(() => {
  const at = index.value
  if (!at || !at.callers.size) return null
  const ids = changedIds(props.files)
  const named = [...ids].filter(id => at.functions.get(id) && !at.functions.get(id)!.fn.anonymous && !at.functions.get(id)!.fn.test)
  const callers = namedOnly(at, blastRadius(at, ids, 2)).size
  // Tests are read in the TypeScript family only; a function in PHP, Rust or Lua is not counted as untested.
  const readable = named.filter(id => TESTS_READ.test(at.functions.get(id)!.file.path))
  const reach = testsReaching(readable, id => (at.callers.get(id) ?? []).map(edge => edge.from), id => !!at.functions.get(id)?.fn.test)
  const busy = busyFiles(props.files.map(file => file.path), hotspots.all.value ?? {})
  return riskOf({ changed: named.length, unreached: reach.unreached.length, unread: named.length - readable.length, callers, busy, days: hotspots.days })
})
const TESTS_READ = /\.(?:[cm]?[jt]sx?|vue|svelte|astro)$/
const COLOR = { low: 'success', medium: 'warning', high: 'error' } as const
</script>

<template>
  <p v-if="risk && (risk.reasons.length || files.length)" class="flex min-w-0 items-center gap-2 text-xs">
    <UBadge :label="`${risk.level} risk`" :color="COLOR[risk.level]" variant="subtle" size="sm" class="shrink-0" />
    <span class="min-w-0 truncate text-muted" :title="`${risk.reasons.join('; ')}. Based on callers, tests and the last ${hotspots.days} days of changes: a hint for where to look first.`">{{ risk.reasons.join(' · ') || 'nothing calls it, and it touches no busy file' }}</span>
  </p>
</template>
