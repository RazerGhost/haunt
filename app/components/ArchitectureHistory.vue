<script setup lang="ts">
import type { ArchitectureHistory } from '~/bindings'

/*
 * The rule breaks over time: for the last commits along the current branch, the imports each one added and removed
 * that break the project's rules (src-tauri/src/code/architecture.rs), as bars (up: added, down: removed), and the
 * total there was after each, worked back from the total now. Read when asked: every version of every changed file is
 * parsed. Click a bar to see that commit's Architecture lens.
 */
const props = defineProps<{ now: number }>()
const { repo } = useRepositories()
const router = useRouter()
const { times } = usePreferences()

const COMMITS = 50
const history = shallowRef<ArchitectureHistory | null>(null)
const problem = ref('')
// A slow answer for the tab before lands nowhere, and does not keep the new tab from asking.
const ask = latest(repo)
const { loading } = ask
async function load(): Promise<void> {
  if (!repo.value || loading.value) return
  problem.value = ''
  try {
    const found = await ask.run(at => haunt.architectureHistory(at, COMMITS))
    if (found !== undefined) history.value = found
  } catch (error) {
    problem.value = errorMessage(error, 'The history could not be read.')
  }
}
watch(repo, () => {
  history.value = null
  ask.cancel()
})

/** Oldest first, each with the total after it (worked back from now). */
const bars = computed(() => {
  const commits = history.value?.commits ?? []
  let after = props.now
  const out = commits.map((commit) => {
    const bar = { ...commit, after }
    after = after - commit.addedBreaks + commit.removedBreaks
    return bar
  })
  return out.reverse()
})
const top = computed(() => Math.max(1, ...bars.value.map(bar => Math.max(bar.addedBreaks, bar.removedBreaks))))
const peak = computed(() => Math.max(1, ...bars.value.map(bar => bar.after)))
const changed = computed(() => bars.value.filter(bar => bar.addedBreaks || bar.removedBreaks).length)
const when = (seconds: number) => times.value === 'date' ? new Date(seconds * 1000).toLocaleString() : new Date(seconds * 1000).toLocaleDateString()
const tip = (bar: (typeof bars.value)[number]) => bar.skipped
  ? `${bar.subject}\n${when(bar.time)}\nNot read: too many files (or a first commit).`
  : `${bar.subject}\n${when(bar.time)}\n+${bar.addedBreaks} rule breaks, −${bar.removedBreaks}; ${bar.after} after it\n${bar.added} imports added, ${bar.removed} removed`
</script>

<template>
  <section class="grid gap-2 border-b border-default px-4 py-3">
    <div class="flex flex-wrap items-center gap-3">
      <h2 class="text-sm font-semibold text-highlighted">Rule breaks over time</h2>
      <p class="min-w-0 flex-1 text-xs text-muted">
        <template v-if="history">Last {{ bars.length }} commits: {{ changed }} changed the rule breaks. Up: added, down: removed; the line is the running total. Click a bar for its imports.</template>
        <template v-else>Rule breaks added and removed by each of the last {{ COMMITS }} commits. Takes a moment.</template>
      </p>
      <UButton :label="history ? 'Read again' : 'Count them'" icon="i-lucide-history" size="xs" color="neutral" variant="outline" :loading="loading" @click="load" />
    </div>
    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <p v-else-if="history && !history.judged" class="text-xs text-muted">No rules check imports here, so there is nothing to count.</p>
    <div v-else-if="history" class="relative h-28" :style="{ width: `min(100%, ${bars.length * 1.5}rem)` }">
      <svg class="absolute inset-0 h-full w-full overflow-visible" :viewBox="`0 0 ${bars.length * 10} 100`" preserveAspectRatio="none" aria-hidden="true">
        <line x1="0" :x2="bars.length * 10" y1="50" y2="50" class="stroke-(--ui-border)" stroke-width="0.5" vector-effect="non-scaling-stroke" />
        <polyline
          :points="bars.map((bar, at) => `${at * 10 + 5},${96 - (bar.after / peak) * 92}`).join(' ')"
          fill="none"
          class="stroke-(--ui-text-muted)"
          stroke-width="1.25"
          vector-effect="non-scaling-stroke"
        />
      </svg>
      <div class="absolute inset-0 grid" :style="{ gridTemplateColumns: `repeat(${bars.length}, minmax(0, 1fr))` }">
        <button
          v-for="bar in bars"
          :key="bar.sha"
          type="button"
          class="group relative h-full hover:bg-elevated/60"
          :title="tip(bar)"
          :aria-label="tip(bar)"
          @click="router.push({ path: '/commits', query: { commit: bar.sha } })"
        >
          <span v-if="bar.skipped" class="absolute top-1/2 left-1/2 size-1 -translate-1/2 rounded-full bg-(--ui-text-dimmed)" />
          <span v-if="bar.addedBreaks" class="absolute bottom-1/2 left-1/4 w-1/2 rounded-t-sm bg-error" :style="{ height: `${(bar.addedBreaks / top) * 46}%` }" />
          <span v-if="bar.removedBreaks" class="absolute top-1/2 left-1/4 w-1/2 rounded-b-sm bg-success" :style="{ height: `${(bar.removedBreaks / top) * 46}%` }" />
        </button>
      </div>
    </div>
  </section>
</template>
