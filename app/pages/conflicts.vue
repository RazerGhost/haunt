<script setup lang="ts">
import type { ConflictFile } from '~/bindings'
import { choose, conflictsLeft, joinSegments, parseConflicts, type Choice, type Segment } from '~~/shared/conflicts'

/*
 * The conflict view: the files that still conflict on the left; on the right, the picked file as blocks. Each block
 * shows your lines and theirs side by side, with a choice: yours, theirs or both. A whole side can be taken at once
 * (the only way for a binary file, or one that a side deleted), and the result can be edited as text. Marking it
 * resolved saves the result and stages it; once no file conflicts, Continue (in the banner) finishes the operation.
 */
const { operation, busy: operating } = useOperation()
const { open: openInEditor } = useCode()
const { repo } = useRepositories()
const route = useRoute()
const router = useRouter()
const dialog = useDialog()
const toast = useToast()
const failure = useFailure()

const files = computed(() => operation.value?.conflicted ?? [])
const picked = computed(() => {
  const asked = typeof route.query.path === 'string' ? route.query.path : null
  return asked && files.value.includes(asked) ? asked : files.value[0] ?? null
})

const file = shallowRef<ConflictFile | null>(null)
const segments = shallowRef<Segment[]>([])
const editing = ref(false)
const text = ref('')
const problem = ref('')
const saving = ref(false)
const ask = latest(repo)

watch(picked, async (path) => {
  file.value = null
  segments.value = []
  editing.value = false
  problem.value = ''
  if (!path) return
  try {
    const found = await ask.run(at => haunt.conflict(at, path))
    if (found === undefined) return
    file.value = found
    segments.value = found.result !== null && !found.binary ? parseConflicts(found.result) : []
    text.value = found.result ?? ''
  } catch (error) {
    problem.value = errorMessage(error, t('pages.conflicts.notRead'))
  }
}, { immediate: true })

const left = computed(() => conflictsLeft(segments.value))
function pick(index: number, choice: Choice): void {
  segments.value = choose(segments.value, index, choice)
  text.value = joinSegments(segments.value)
}
/** The blocks numbered among the conflicts only, for `choose`. */
const numbered = computed(() => {
  let count = -1
  return segments.value.map(segment => ({ segment, index: segment.kind === 'conflict' ? ++count : -1 }))
})
/** Plain text between blocks, cut to its first and last lines when long. */
function context(value: string): string {
  const lines = value.replace(/\n$/, '').split('\n')
  if (lines.length <= 7) return lines.join('\n')
  return [...lines.slice(0, 3), t('pages.conflicts.moreLines', { count: lines.length - 6 }), ...lines.slice(-3)].join('\n')
}
const deletedBy = computed(() => {
  if (!file.value) return null
  if (file.value.ours === null) return 'yours'
  if (file.value.theirs === null) return 'theirs'
  return null
})

async function save(write: (at: string) => Promise<unknown>): Promise<void> {
  if (!repo.value) return
  saving.value = true
  try {
    await write(repo.value)
    toast.add({ title: t('pages.conflicts.resolved', { path: picked.value ?? '' }), icon: 'i-lucide-check' })
    // The next file, if any; the list comes back on the git news.
    const next = files.value.find(path => path !== picked.value)
    void router.replace({ query: { ...route.query, path: next } })
  } catch (error) {
    failure.failed(t('pages.conflicts.notResolved'), error, t('common.tryAgain'))
  } finally {
    saving.value = false
  }
}
async function resolve(): Promise<void> {
  const result = editing.value ? text.value : joinSegments(segments.value)
  if (/^<{7}( |$)/m.test(result) && /^>{7}( |$)/m.test(result)) {
    const ok = await dialog.confirm({ title: t('pages.conflicts.markersTitle'), description: t('pages.conflicts.markersDescription'), confirm: t('pages.conflicts.markResolved'), danger: true })
    if (!ok) return
  }
  await save(at => haunt.resolve(at, picked.value!, result))
}
const take = (side: 'ours' | 'theirs') => save(at => haunt.takeSide(at, picked.value!, side))

// Next and previous conflict: the blocks still to choose, scrolled to one after another.
const blocks = ref<HTMLElement[]>([])
const at = ref(-1)
watch(left, () => { at.value = -1 })
function go(step: number): void {
  const list = blocks.value.filter(Boolean)
  if (!list.length) return
  at.value = (at.value + step + list.length) % list.length
  list[at.value]!.scrollIntoView({ behavior: 'smooth', block: 'center' })
}
</script>

<template>
  <div v-if="!operation" class="grid justify-items-center gap-3 py-16 text-muted">
    <UIcon name="i-lucide-check-circle" class="size-6" />
    <p>{{ t('pages.conflicts.nothing') }}</p>
    <UButton :label="t('pages.conflicts.allCommits')" to="/commits" size="sm" color="neutral" variant="outline" />
  </div>
  <div v-else class="grid h-full grid-cols-[16rem_minmax(0,1fr)]">
    <aside class="flex min-h-0 flex-col border-r border-default">
      <p class="border-b border-default px-3 py-2 text-label font-semibold tracking-wider text-muted uppercase">{{ t('pages.conflicts.conflicts') }} <span class="tabular-nums">{{ files.length }}</span></p>
      <ul class="min-h-0 flex-1 overflow-auto p-1">
        <li v-for="path in files" :key="path">
          <NuxtLink
            :to="{ query: { ...route.query, path } }"
            class="flex items-center gap-1.5 rounded px-2 py-1 text-ui"
            :class="path === picked ? 'bg-elevated text-highlighted' : 'hover:bg-elevated/60'"
            :title="path"
          >
            <span class="w-3 shrink-0 text-center font-mono text-label font-semibold text-error">!</span>
            <span class="min-w-0 truncate">{{ path }}</span>
          </NuxtLink>
        </li>
      </ul>
      <p v-if="!files.length" class="p-3 text-ui text-muted">{{ t('pages.conflicts.noneLeft') }}</p>
    </aside>

    <section v-if="picked" class="flex min-h-0 flex-col">
      <header class="flex flex-wrap items-center gap-2 border-b border-default px-4 py-2">
        <span class="min-w-0 flex-1 truncate font-mono text-ui font-medium text-highlighted">{{ picked }}</span>
        <UButton icon="i-lucide-square-arrow-out-up-right" :label="t('pages.conflicts.openInEditor')" size="xs" color="neutral" variant="ghost" @click="openInEditor(picked!)" />
        <UButton :label="t('pages.conflicts.takeOurs', { name: said(operation.ours) })" size="xs" color="neutral" variant="outline" :disabled="saving || operating" @click="take('ours')" />
        <UButton :label="t('pages.conflicts.takeTheirs', { name: said(operation.theirs) })" size="xs" color="neutral" variant="outline" :disabled="saving || operating" @click="take('theirs')" />
      </header>

      <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" class="m-4" />
      <div v-else-if="!file" class="grid justify-items-center py-10"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
      <div v-else-if="file.binary || deletedBy || file.result === null" class="p-4 text-ui text-muted">
        <p v-if="deletedBy">{{ deletedBy === 'yours' ? t('pages.conflicts.deletedOurs', { name: said(operation.ours) }) : t('pages.conflicts.deletedTheirs', { name: said(operation.theirs) }) }}</p>
        <p v-else>{{ t('pages.conflicts.binary') }}</p>
      </div>
      <template v-else>
        <div class="flex items-center gap-3 px-4 pt-3 text-xs text-muted">
          <span>{{ left ? t('pages.conflicts.blocksLeft', { count: left }) : t('pages.conflicts.allChosen') }}</span>
          <UFieldGroup v-if="left && !editing" size="xs">
            <UButton icon="i-lucide-chevron-up" color="neutral" variant="outline" :aria-label="t('pages.conflicts.previous')" :title="t('pages.conflicts.previous')" @click="go(-1)" />
            <UButton icon="i-lucide-chevron-down" color="neutral" variant="outline" :aria-label="t('pages.conflicts.next')" :title="t('pages.conflicts.next')" @click="go(1)" />
          </UFieldGroup>
          <USwitch v-model="editing" :label="t('pages.conflicts.editAsText')" size="xs" class="ml-auto" @update:model-value="(on: boolean) => { if (on) text = joinSegments(segments) }" />
        </div>
        <div v-if="editing" class="min-h-0 flex-1 p-4">
          <UTextarea v-model="text" :rows="20" autoresize class="w-full" :ui="{ base: 'font-mono text-xs' }" />
        </div>
        <div v-else class="grid min-h-0 flex-1 content-start gap-2 overflow-auto p-4">
          <template v-for="(item, at) in numbered" :key="at">
            <pre v-if="item.segment.kind === 'text'" class="overflow-x-auto px-2 font-mono text-xs whitespace-pre text-muted">{{ context(item.segment.text) }}</pre>
            <div v-else ref="blocks" class="grid gap-2 rounded-md border border-warning/40 p-2">
              <div class="grid grid-cols-2 gap-2">
                <div class="min-w-0">
                  <p class="mb-1 text-label font-semibold text-muted uppercase">{{ t('pages.conflicts.yours') }} · {{ said(operation.ours) }}</p>
                  <CodeLines v-if="item.segment.ours" :path="picked!" :text="item.segment.ours" class="rounded bg-success/10 p-2" />
                  <p v-else class="rounded bg-success/10 p-2 text-xs text-muted">{{ t('pages.conflicts.empty') }}</p>
                </div>
                <div class="min-w-0">
                  <p class="mb-1 text-label font-semibold text-muted uppercase">{{ t('pages.conflicts.theirs') }} · {{ said(operation.theirs) }}</p>
                  <CodeLines v-if="item.segment.theirs" :path="picked!" :text="item.segment.theirs" class="rounded bg-info/10 p-2" />
                  <p v-else class="rounded bg-info/10 p-2 text-xs text-muted">{{ t('pages.conflicts.empty') }}</p>
                </div>
              </div>
              <div class="flex gap-1.5">
                <UButton :label="t('pages.conflicts.useOurs')" size="xs" color="neutral" variant="outline" @click="pick(item.index, 'ours')" />
                <UButton :label="t('pages.conflicts.useTheirs')" size="xs" color="neutral" variant="outline" @click="pick(item.index, 'theirs')" />
                <UButton :label="t('pages.conflicts.useBoth')" size="xs" color="neutral" variant="outline" @click="pick(item.index, 'both')" />
              </div>
            </div>
          </template>
        </div>
        <footer class="flex justify-end gap-2 border-t border-default px-4 py-2">
          <UButton :label="t('pages.conflicts.markAsResolved')" icon="i-lucide-check" size="sm" color="primary" :loading="saving" :disabled="operating" @click="resolve" />
        </footer>
      </template>
    </section>
  </div>
</template>
