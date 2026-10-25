<script setup lang="ts">
import type { ChangedFunction, Changes } from '~/bindings'

/*
 * Reviewing a branch, function by function: the functions your branch changed since it left main (its commits and
 * the work not committed yet), one at a time, each with its code as it is now, its callers (what could break), and
 * the file's diff since the branch left main. ← and → (or J and K) go through them; Seen marks the ones looked at.
 */
const { repo, gitChanged } = useRepositories()
const { index, select } = useCode()
const route = useRoute()
const router = useRouter()

const changes = shallowRef<Changes | null>(null)
const problem = ref('')
const ask = latest(repo)
async function load(): Promise<void> {
  try {
    const found = await ask.run(at => haunt.changes(at, { kind: 'merge-base', branch: null }))
    if (found !== undefined) changes.value = found
  } catch (error) {
    problem.value = errorMessage(error, 'The branch\'s changes could not be read.')
  }
}
watch([repo, gitChanged('head', 'index')], () => repo.value && void load(), { immediate: true })

/** The changed functions still in the code, file by file. */
const items = computed(() => (changes.value?.files ?? []).flatMap(file => file.functions
  .filter((fn): fn is ChangedFunction & { id: string } => !!fn.id && fn.status !== 'removed')
  .map(fn => ({ file, fn }))))
const at = computed(() => Math.min(items.value.length - 1, Math.max(0, Number(route.query.at ?? 0) || 0)))
const item = computed(() => items.value[at.value] ?? null)
function go(step: number): void {
  const next = Math.min(items.value.length - 1, Math.max(0, at.value + step))
  void router.replace({ query: { ...route.query, at: String(next) } })
}
const seen = ref(new Set<string>())
function markSeen(): void {
  if (!item.value) return
  seen.value = new Set([...seen.value, item.value.fn.id])
  if (at.value < items.value.length - 1) go(1)
}
const callers = computed(() => (item.value ? index.value?.callers.get(item.value.fn.id) ?? [] : []))
const reachFrom = computed(() => new Set(item.value ? [item.value.fn.id] : []))
const located = computed(() => (item.value ? index.value?.functions.get(item.value.fn.id) ?? null : null))

function onKey(event: KeyboardEvent): void {
  const target = event.target as HTMLElement | null
  if (event.ctrlKey || event.metaKey || event.altKey || target?.closest?.('input, textarea, [role=dialog]')) return
  if (event.key === 'ArrowRight' || event.key === 'j') go(1)
  else if (event.key === 'ArrowLeft' || event.key === 'k') go(-1)
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
const STATUS: Record<string, string> = { added: 'success', changed: 'warning', removed: 'error', moved: 'info' }
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex flex-wrap items-center gap-2 border-b border-default px-4 py-2">
      <div class="min-w-0 flex-1">
        <h1 class="truncate text-sm font-semibold text-highlighted">Review {{ changes?.head.branch ?? 'this branch' }}</h1>
        <p class="truncate text-xs text-muted">{{ changes ? `Against ${changes.base.label}: ${items.length} changed ${items.length === 1 ? 'function' : 'functions'}, ${seen.size} seen. ← → or J K to go through them.` : 'Reading the changes…' }}</p>
      </div>
      <template v-if="items.length">
        <UButton icon="i-lucide-chevron-left" size="xs" color="neutral" variant="outline" :disabled="at === 0" aria-label="Previous function" @click="go(-1)" />
        <span class="text-xs text-muted tabular-nums">{{ at + 1 }} / {{ items.length }}</span>
        <UButton icon="i-lucide-chevron-right" size="xs" color="neutral" variant="outline" :disabled="at === items.length - 1" aria-label="Next function" @click="go(1)" />
        <UButton :label="item && seen.has(item.fn.id) ? 'Seen' : 'Mark seen'" icon="i-lucide-check" size="xs" :color="item && seen.has(item.fn.id) ? 'success' : 'primary'" :variant="item && seen.has(item.fn.id) ? 'soft' : 'solid'" @click="markSeen" />
      </template>
    </div>
    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" class="m-4" />
    <p v-else-if="changes && !items.length" class="p-6 text-center text-sm text-muted">This branch changes no functions since {{ changes.base.label }}.</p>
    <div v-else-if="item" class="grid min-h-0 flex-1 grid-cols-[16rem_minmax(0,1fr)]">
      <ul class="min-h-0 overflow-auto border-r border-default p-1">
        <li v-for="(entry, number) in items" :key="entry.fn.id">
          <button
            type="button"
            class="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-xs"
            :class="number === at ? 'bg-elevated text-highlighted' : 'hover:bg-elevated/60'"
            @click="router.replace({ query: { ...route.query, at: String(number) } })"
          >
            <UIcon :name="seen.has(entry.fn.id) ? 'i-lucide-check' : 'i-lucide-dot'" class="size-3.5 shrink-0" :class="seen.has(entry.fn.id) ? 'text-success' : 'text-dimmed'" />
            <span class="min-w-0 flex-1 truncate font-mono">{{ entry.fn.qualified }}</span>
            <span class="shrink-0 text-label" :class="`text-${STATUS[entry.fn.status] ?? 'muted'}`">{{ entry.fn.status }}</span>
          </button>
        </li>
      </ul>
      <div class="grid min-h-0 content-start gap-4 overflow-auto p-4">
        <div class="flex flex-wrap items-center gap-2">
          <span class="font-mono text-base font-semibold text-highlighted">{{ item.fn.qualified }}</span>
          <UBadge :label="item.fn.status" :color="(STATUS[item.fn.status] ?? 'neutral') as 'success' | 'warning' | 'error' | 'neutral'" variant="subtle" size="sm" />
          <span v-if="item.fn.from" class="font-mono text-xs text-info">from {{ item.fn.from.name }} in {{ item.fn.from.path }}</span>
          <span class="text-xs text-success">+{{ item.fn.added }}</span><span class="text-xs text-error">−{{ item.fn.removed }}</span>
          <button type="button" class="font-mono text-xs text-muted hover:underline" @click="select(item.fn.id)">{{ item.file.path }}:{{ item.fn.line }}</button>
        </div>
        <FunctionCode v-if="located" :path="located.file.path" :start="located.fn.line" :end="located.fn.endLine" />
        <section class="grid gap-1">
          <h2 class="text-label font-semibold tracking-wider text-muted uppercase">Called by {{ callers.length }}</h2>
          <p v-if="!callers.length" class="text-xs text-muted">Nothing in the scan calls it.</p>
          <ul v-else class="grid gap-0.5">
            <li v-for="edge in callers.slice(0, 30)" :key="edge.from">
              <button type="button" class="flex w-full items-center gap-2 rounded px-2 py-0.5 text-left text-xs hover:bg-elevated" @click="select(edge.from)">
                <span class="font-mono">{{ index?.functions.get(edge.from)?.fn.name ?? edge.from }}</span>
                <span class="min-w-0 truncate text-muted">{{ index?.functions.get(edge.from)?.file.path }}</span>
              </button>
            </li>
          </ul>
        </section>
        <TestsReached :changed="reachFrom" />
        <section v-if="changes?.head.sha" class="grid gap-1">
          <h2 class="text-label font-semibold tracking-wider text-muted uppercase">The file since {{ changes.base.label }} (committed)</h2>
          <DiffView :key="`${changes.base.sha}:${item.file.path}`" :from="changes.base.sha" :to="changes.head.sha" :path="item.file.path" :old-path="item.file.oldPath" class="max-h-[32rem] rounded-md border border-default" />
        </section>
      </div>
    </div>
  </div>
</template>
