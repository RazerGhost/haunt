<script setup lang="ts">
import { GraphLayout, type GraphRow } from '~~/shared/graph'
import type { LogCommit, LogQuery } from '~~/shared/types'

/*
 * The commit list with its graph, newest first. Pages of 300 load as you scroll; only the rows on screen are drawn, so
 * a long history stays quick. Click a commit to see it; Ctrl-click (⌘ on macOS) a second one to compare the two.
 * Up and down move the selection. Read again after a commit, fetch or branch switch, keeping the rows already loaded.
 */
const props = defineProps<{ from?: string, author?: string, message?: string, path?: string, changed?: string, merges?: 'only' | 'none', selected: string | null, compare: string | null }>()
const emit = defineEmits<{ select: [sha: string, compare: boolean] }>()
const { repo, gitChanged } = useRepositories()
// A commit, a switch or a fetch; staging alone does not change the list.
const listChanged = gitChanged('head', 'refs')

const ROW = 26
const PAGE = 300
const commits = shallowRef<LogCommit[]>([])
const rows = shallowRef<GraphRow[]>([])
const more = ref(false)
const problem = ref('')
let layout = new GraphLayout()
const ask = latest(repo)
const { loading } = ask
// How times show and in what order commits are listed (Settings, General).
const { times, order } = usePreferences()

// A right click on a row: a branch or a tag on that commit, or its sha to copy.
const refActions = useRefActions()
const undo = useOperation()
const webs = useWeb()
const { current, localChanges } = useRefs()
const patches = usePatches()
/** A commit's whole message (subject and body), for the menu's Copy the message. */
async function copyMessage(sha: string): Promise<void> {
  if (!repo.value) return
  try {
    const detail = await haunt.commit(repo.value, sha)
    await navigator.clipboard?.writeText(detail.body ? `${detail.subject}\n\n${detail.body}` : detail.subject)
  } catch { /* no clipboard, or the commit is gone: nothing to do */ }
}
const menuFor = shallowRef<{ sha: string, subject: string, parents: string[], refs: { kind: string }[] } | null>(null)
const menu = computed(() => {
  const commit = menuFor.value
  if (!commit) return []
  const at = { rev: commit.sha, label: `${commit.sha.slice(0, 7)} (${commit.subject})` }
  return [
    [
      { label: t('CommitList.newBranch'), icon: 'i-lucide-git-branch-plus', onSelect: () => refActions.newBranch(at) },
      { label: t('CommitList.newTag'), icon: 'i-lucide-tag', onSelect: () => refActions.newTag(at) }
    ],
    [
      { label: current.value ? t('CommitList.cherryPick', { branch: current.value.name }) : t('CommitList.cherryPickCurrent'), icon: 'i-lucide-cherry', onSelect: () => undo.cherryPick(commit.sha, commit.subject) },
      { label: t('CommitList.revert'), icon: 'i-lucide-undo-2', onSelect: () => undo.revert(commit.sha, commit.subject) },
      // The commit HEAD is on: undo it; any other: reset the branch to it.
      commit.refs.some(ref => ref.kind === 'head')
        ? { label: t('CommitList.undoCommit'), icon: 'i-lucide-rotate-ccw', onSelect: () => undo.undoCommit(commit.subject) }
        : { label: current.value ? t('CommitList.reset', { branch: current.value.name }) : t('CommitList.resetCurrent'), icon: 'i-lucide-history', onSelect: () => undo.reset(commit.sha, at.label) },
      ...commit.refs.some(ref => ref.kind === 'head') ? [] : [{ label: t('CommitList.rebase'), icon: 'i-lucide-git-pull-request-arrow', onSelect: () => navigateTo({ path: '/rebase', query: { onto: commit.sha } }) }],
      ...commit.refs.some(ref => ref.kind === 'head') ? [] : [{ label: t('CommitList.bisectGood'), icon: 'i-lucide-crosshair', onSelect: () => navigateTo({ path: '/bisect', query: { good: commit.sha } }) }]
    ],
    [
      { label: t('CommitList.searchFiles'), icon: 'i-lucide-text-search', onSelect: () => navigateTo({ path: '/grep', query: { at: commit.sha } }) },
      { label: t('CommitList.copySha'), icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(commit.sha) },
      { label: t('CommitList.copyMessage'), icon: 'i-lucide-copy', onSelect: () => copyMessage(commit.sha) },
      { label: t('CommitList.savePatch'), icon: 'i-lucide-file-diff', onSelect: () => patches.save(commit.sha) },
      // It and the commits after it, up to where you are (a first commit has nothing before it to start after).
      ...commit.parents.length && !commit.refs.some(ref => ref.kind === 'head') ? [{ label: t('CommitList.savePatches'), icon: 'i-lucide-files', onSelect: () => patches.save('HEAD', commit.parents[0]!) }] : [],
      ...webs.label.value ? [{ label: webs.label.value, icon: 'i-lucide-external-link', onSelect: () => webs.openCommit(commit.sha) }] : [],
      ...webs.copyLabel.value ? [{ label: webs.copyLabel.value, icon: 'i-lucide-link', onSelect: () => webs.copyCommit(commit.sha) }] : []
    ]
  ]
})
const list = useTemplateRef<HTMLElement>('list')
const scrollTop = ref(0)
const height = ref(600)

/** The next page, or (`fresh`) the list from the top, as many rows as `limit`. */
async function page(fresh: boolean, limit = PAGE): Promise<void> {
  if (loading.value && !fresh) return
  try {
    const query: LogQuery = { from: props.from ? [props.from] : undefined, author: props.author || undefined, message: props.message || undefined, path: props.path || undefined, by_date: order.value === 'date', merges: props.merges ?? null, changed: props.changed || null, skip: fresh ? 0 : commits.value.length, limit }
    const found = await ask.run(at => haunt.log(at, query))
    if (found === undefined) return
    if (fresh) layout = new GraphLayout()
    commits.value = fresh ? found.commits : [...commits.value, ...found.commits]
    rows.value = fresh ? layout.add(found.commits) : [...rows.value, ...layout.add(found.commits)]
    more.value = found.more
    problem.value = ''
  } catch (error) {
    problem.value = errorMessage(error, t('CommitList.readError'))
  }
}
watch(() => [props.from, props.author, props.message, props.path, props.changed, props.merges, repo.value, order.value], () => {
  commits.value = []
  rows.value = []
  scrollTop.value = 0
  if (list.value) list.value.scrollTop = 0
  void page(true)
}, { immediate: true })
// A commit, fetch or branch switch: read the list again, as many rows as were loaded, so the place stays.
watch(listChanged, () => void page(true, Math.max(PAGE, commits.value.length)))

let observer: ResizeObserver | null = null
onMounted(() => {
  observer = new ResizeObserver(([entry]) => { height.value = entry?.contentRect.height ?? 600 })
  if (list.value) observer.observe(list.value)
})
onBeforeUnmount(() => observer?.disconnect())

function onScroll(): void {
  scrollTop.value = list.value?.scrollTop ?? 0
  // Near the end: the next page.
  if (more.value && !loading.value && scrollTop.value + height.value > (commits.value.length - 60) * ROW) void page(false)
}

const visible = computed(() => {
  const start = Math.max(0, Math.floor(scrollTop.value / ROW) - 10)
  const end = Math.min(commits.value.length, Math.ceil((scrollTop.value + height.value) / ROW) + 10)
  return Array.from({ length: Math.max(0, end - start) }, (_, k) => start + k)
})
/** A row's graph is as wide as the lanes passing through it (at most 100), so its text starts right after them and
 * moves only when its own lanes do. */
const lanesOf = (at: number) => Math.min(100, Math.max(1, rows.value[at]?.width ?? 1))

function choose(commit: LogCommit, event: MouseEvent): void {
  emit('select', commit.sha, event.ctrlKey || event.metaKey)
}

function move(step: number): void {
  const at = commits.value.findIndex(commit => commit.sha === props.selected)
  const next = commits.value[Math.min(commits.value.length - 1, Math.max(0, at + step))]
  if (!next) return
  emit('select', next.sha, false)
  const index = commits.value.indexOf(next)
  const el = list.value
  if (el && (index * ROW < el.scrollTop || (index + 1) * ROW > el.scrollTop + el.clientHeight)) el.scrollTop = index * ROW - el.clientHeight / 2
}

function when(seconds: number): string {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return t('CommitList.minutes', { count: Math.max(1, minutes) })
  if (minutes < 60 * 24) return t('CommitList.hours', { count: Math.floor(minutes / 60) })
  if (minutes < 60 * 24 * 30) return t('CommitList.days', { count: Math.floor(minutes / 1440) })
  return new Date(seconds * 1000).toLocaleDateString()
}

</script>

<template>
  <div class="flex min-h-0 flex-col">
    <UAlert v-if="problem" class="m-3" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <p v-else-if="!loading && !commits.length" class="p-6 text-center text-sm text-muted">{{ props.author || props.message || props.path || props.changed || props.merges ? t('CommitList.noMatch') : t('CommitList.noCommits') }}</p>
    <!-- The work not committed yet, above the history: one click to Local changes. -->
    <NuxtLink
      v-if="localChanges && !props.from && !props.author && !props.message && !props.path && !props.changed && !props.merges"
      to="/local"
      class="flex shrink-0 items-center gap-2 border-b border-default px-3 py-1.5 text-ui hover:bg-elevated/60"
    >
      <span class="grid size-3.5 shrink-0 place-items-center rounded-full border-2 border-dashed border-warning" aria-hidden="true" />
      <span class="font-medium text-highlighted">{{ t('CommitList.localChanges') }}</span>
      <span class="text-muted">{{ current ? t('CommitList.notCommittedOn', { count: localChanges, branch: current.name }) : t('CommitList.notCommitted', { count: localChanges }) }}</span>
      <UIcon name="i-lucide-chevron-right" class="ml-auto size-3.5 text-dimmed" />
    </NuxtLink>
    <UContextMenu :items="menu" :disabled="!menuFor">
    <div
      ref="list"
      class="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-(--ui-primary)"
      tabindex="0"
      role="listbox"
      :aria-label="t('CommitList.commits')"
      @scroll.passive="onScroll"
      @keydown.down.prevent="move(1)"
      @keydown.up.prevent="move(-1)"
    >
      <div :style="{ height: `${commits.length * ROW}px` }" class="relative">
        <div
          v-for="at in visible"
          :key="commits[at]!.sha"
          role="option"
          :aria-selected="commits[at]!.sha === props.selected"
          class="absolute inset-x-0 flex cursor-default items-center gap-2 pr-3 pl-2 text-xs"
          :class="commits[at]!.sha === props.selected ? 'bg-elevated' : commits[at]!.sha === props.compare ? 'bg-elevated/60 ring-1 ring-inset ring-(--ui-border-accented)' : 'hover:bg-elevated/50'"
          :style="{ top: `${at * ROW}px`, height: `${ROW}px` }"
          @click="choose(commits[at]!, $event)"
          @contextmenu="menuFor = commits[at]!"
        >
          <GraphCell :row="rows[at]!" :width="lanesOf(at)" :merge="commits[at]!.parents.length > 1" :head="commits[at]!.refs.some(ref => ref.kind === 'head')" />
          <span class="flex min-w-0 flex-1 items-center gap-1.5">
            <template v-for="ref in commits[at]!.refs.filter(r => r.kind !== 'head')" :key="`${ref.kind}:${ref.name}`">
              <span
                class="flex shrink-0 items-center gap-0.5 rounded px-1.5 text-label leading-4 font-medium"
                :class="ref.kind === 'branch' ? 'bg-(--ui-primary)/10 text-highlighted' : ref.kind === 'remote' ? 'border border-default text-muted' : 'bg-warning/15 text-warning'"
                :title="ref.kind === 'branch' ? t('CommitList.branch') : ref.kind === 'remote' ? t('CommitList.remoteBranch') : t('CommitList.tag')"
              >
                <UIcon v-if="ref.kind === 'tag'" name="i-lucide-tag" class="size-2.5" />
                <UIcon v-else-if="ref.kind === 'remote'" name="i-lucide-cloud" class="size-2.5" />
                <span :class="{ 'font-bold': ref.kind === 'branch' && commits[at]!.refs.some(r => r.kind === 'head') }">{{ ref.name }}</span>
              </span>
            </template>
            <span class="truncate" :class="commits[at]!.sha === props.selected ? 'text-highlighted' : ''" :title="commits[at]!.subject">{{ commits[at]!.subject }}</span>
          </span>
          <UIcon v-if="commits[at]!.signed" name="i-lucide-shield-check" class="size-3.5 shrink-0 text-dimmed" :title="t('CommitList.signed')" />
          <span class="w-36 shrink-0 truncate text-muted" :title="commits[at]!.email">{{ commits[at]!.author }}</span>
          <span class="shrink-0 text-right text-muted tabular-nums" :class="times === 'date' ? 'w-32' : 'w-20'" :title="new Date(commits[at]!.time * 1000).toLocaleString()">{{ when(commits[at]!.time) }}</span>
          <span class="w-16 shrink-0 text-right font-mono text-dimmed">{{ commits[at]!.sha.slice(0, 7) }}</span>
        </div>
      </div>
      <div v-if="loading" class="sticky bottom-0 flex justify-center bg-default/80 py-1 text-xs text-muted">
        <UIcon name="i-lucide-loader-circle" class="size-3.5 animate-spin" />
      </div>
    </div>
    </UContextMenu>
  </div>
</template>
