<script setup lang="ts">
/*
 * The commit list: filters on top, the commits with their graph, and the picked commit below with its
 * lenses. The filters and the picked commits are in the address (?from, author, message, path, commit, compare), so
 * each tab comes back to them.
 */
const route = useRoute()
const router = useRouter()
const { refs } = useRefs()

const param = (name: string) => (typeof route.query[name] === 'string' ? route.query[name] as string : '')
const from = computed(() => param('from'))
const author = computed(() => param('author'))
const message = computed(() => param('message'))
const path = computed(() => param('path'))
const changed = computed(() => param('changed'))
const merges = computed(() => (param('merges') === 'only' || param('merges') === 'none' ? param('merges') as 'only' | 'none' : undefined))
const MERGES = [{ value: 'all', label: 'All commits' }, { value: 'none', label: 'No merges' }, { value: 'only', label: 'Merges only' }]
const mergeChoice = computed({ get: () => merges.value ?? 'all', set: (value: string) => set({ merges: value === 'all' ? null : value }) })
const selected = computed(() => param('commit') || null)
const compare = computed(() => param('compare') || null)

function set(values: Record<string, string | null>): void {
  const query = { ...route.query }
  for (const [key, value] of Object.entries(values)) {
    if (value) query[key] = value
    else delete query[key]
  }
  void router.replace({ query })
}

// The select needs a value for "every branch"; no branch can be called this.
const ALL = '*all*'
const branches = computed(() => [
  { value: ALL, label: 'All branches' },
  ...(refs.value?.branches.map(branch => ({ value: branch.name, label: branch.name })) ?? []),
  ...(refs.value?.remotes.flatMap(remote => remote.branches.map(branch => ({ value: `${remote.name}/${branch.name}`, label: `${remote.name}/${branch.name}` }))) ?? [])
])
const branch = computed({ get: () => from.value || ALL, set: value => set({ from: value === ALL ? null : value }) })

// The text filters apply once you stop typing.
const typed = reactive({ author: author.value, message: message.value, path: path.value, changed: changed.value })
let timer: ReturnType<typeof setTimeout> | null = null
watch(typed, () => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => set({ author: typed.author.trim() || null, message: typed.message.trim() || null, path: typed.path.trim() || null, changed: typed.changed || null }), 350)
})
// A filter set from elsewhere (a link: a function's "every commit that mentions it") shows in its box too.
watch([author, message, path, changed], ([nowAuthor, nowMessage, nowPath, nowChanged]) => {
  if ((typed.author.trim() || '') !== (nowAuthor || '')) typed.author = nowAuthor || ''
  if ((typed.message.trim() || '') !== (nowMessage || '')) typed.message = nowMessage || ''
  if ((typed.path.trim() || '') !== (nowPath || '')) typed.path = nowPath || ''
  if ((typed.changed || '') !== (nowChanged || '')) typed.changed = nowChanged || ''
})
const filtered = computed(() => !!(from.value || author.value || message.value || path.value || changed.value || merges.value))
function clear(): void {
  typed.author = ''
  typed.message = ''
  typed.path = ''
  typed.changed = ''
  set({ from: null, author: null, message: null, path: null, changed: null, merges: null })
}

function pick(sha: string, add: boolean): void {
  if (add && selected.value && sha !== selected.value) set({ compare: sha })
  else set({ commit: sha, compare: null })
}

// The details pane's height, dragged by its top edge, kept between visits.
const share = usePersisted('details-share', 0.5, value => typeof value === 'number' && value >= 0.15 && value <= 0.85)
const area = useTemplateRef<HTMLElement>('area')
function drag(start: PointerEvent): void {
  const box = area.value?.getBoundingClientRect()
  if (!box) return
  const handle = start.target as HTMLElement
  handle.setPointerCapture(start.pointerId)
  const moveTo = (event: PointerEvent) => { share.value = Math.min(0.85, Math.max(0.15, (box.bottom - event.clientY) / box.height)) }
  const stop = () => {
    handle.removeEventListener('pointermove', moveTo)
    handle.removeEventListener('pointerup', stop)
  }
  handle.addEventListener('pointermove', moveTo)
  handle.addEventListener('pointerup', stop)
}
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex flex-wrap items-center gap-2 border-b border-default px-3 py-1.5">
      <USelect v-model="branch" :items="branches" size="xs" class="w-48" aria-label="Branch" />
      <UInput v-model="typed.message" icon="i-lucide-message-square" placeholder="Message" size="xs" class="w-44" aria-label="Filter by message" />
      <UInput v-model="typed.author" icon="i-lucide-user" placeholder="Author" size="xs" class="w-36" aria-label="Filter by author" />
      <UInput v-model="typed.path" icon="i-lucide-file" placeholder="File or folder" size="xs" class="w-48" :ui="{ base: 'font-mono' }" aria-label="Filter by file or folder" />
      <UInput v-model="typed.changed" icon="i-lucide-text-search" placeholder="Text in changes" size="xs" class="w-40" :ui="{ base: 'font-mono' }" title="Commits that added or removed this text" aria-label="Filter by text added or removed" />
      <USelect v-model="mergeChoice" :items="MERGES" size="xs" class="w-32" aria-label="Merges" />
      <UButton v-if="filtered" label="Clear" icon="i-lucide-x" size="xs" color="neutral" variant="ghost" @click="clear" />
      <UButton v-if="refs?.sha" label="HEAD" icon="i-lucide-crosshair" size="xs" color="neutral" variant="ghost" title="Pick the commit you are on" @click="pick(refs.sha!, false)" />
      <span class="ml-auto text-label text-muted">Ctrl-click a second commit to compare · ↑ ↓ to move</span>
    </div>
    <div ref="area" class="flex min-h-0 flex-1 flex-col">
      <CommitList :from="from" :author="author" :message="message" :path="path" :changed="changed" :merges="merges" :selected="selected" :compare="compare" class="min-h-0" :style="{ flex: `${1 - share} 1 0` }" @select="pick" />
      <div
        class="h-1.5 shrink-0 cursor-row-resize border-y border-default bg-elevated/50 hover:bg-accented"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Drag to resize the details"
        @pointerdown.prevent="drag"
      />
      <div class="flex min-h-0 flex-col" :style="{ flex: `${share} 1 0` }">
        <CommitDetails v-if="selected" :sha="selected" :compare="compare" class="min-h-0 flex-1" @select="sha => pick(sha, false)" />
        <p v-else class="grid flex-1 place-items-center text-sm text-muted">Pick a commit to see what it changed.</p>
      </div>
    </div>
  </div>
</template>
