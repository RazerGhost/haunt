<script setup lang="ts">
/*
 * The commit list: filters on top, the commits with their graph, and the picked commit below with its
 * lenses. The filters and the picked commits are in the address (?from, not, author, message, path, commit, compare,
 * landed), so each tab comes back to them. The message filter is typed in the toolbar's search (AppToolbar).
 */
const route = useRoute()
const router = useRouter()
const { refs, current } = useRefs()
const landed = useLanded()

const param = (name: string) => (typeof route.query[name] === 'string' ? route.query[name] as string : '')
const from = computed(() => param('from'))
/** Leaves out what this has: `from=main&not=origin/main` lists the commits only main has. */
const not = computed(() => param('not'))
/** `landed=hide`: the branches the main line already has in full are left out of the whole list. */
const hideLanded = computed({ get: () => param('landed') === 'hide', set: (value: boolean) => set({ landed: value ? 'hide' : null }) })
const author = computed(() => param('author'))
const message = computed(() => param('message'))
const path = computed(() => param('path'))
const changed = computed(() => param('changed'))
const merges = computed(() => (param('merges') === 'only' || param('merges') === 'none' ? param('merges') as 'only' | 'none' : undefined))
const MERGES = computed(() => [{ value: 'all', label: t('pages.commits.allCommits') }, { value: 'none', label: t('pages.commits.noMerges') }, { value: 'only', label: t('pages.commits.mergesOnly') }])
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
// Local branches and remote branches apart, each group with its heading; the menu can be searched.
const branches = computed(() => {
  const remote = refs.value?.remotes.flatMap(one => one.branches.map(branch => `${one.name}/${branch.name}`)) ?? []
  const groups: { type?: 'label', value?: string, label: string }[][] = [[{ value: ALL, label: t('pages.commits.allBranches') }]]
  if (refs.value?.branches.length) groups.push([{ type: 'label', label: t('pages.commits.local') }, ...refs.value.branches.map(branch => ({ value: branch.name, label: branch.name }))])
  if (remote.length) groups.push([{ type: 'label', label: t('pages.commits.remote') }, ...remote.map(name => ({ value: name, label: name }))])
  return groups
})
const branch = computed({ get: () => from.value || ALL, set: (value: string) => set({ from: value === ALL ? null : value, not: null }) })
/** The branch whose distance from its remote branch is worth a word above the list: the one listed, else yours. */
const apartName = computed(() => {
  if (!from.value) return current.value?.name ?? null
  return refs.value?.branches.some(one => one.name === from.value) ? from.value : null
})

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
const filtered = computed(() => !!(from.value || not.value || author.value || message.value || path.value || changed.value || merges.value || hideLanded.value))
/** How many of the filters in More filters are on. */
const moreOn = computed(() => [author.value, path.value, changed.value, merges.value, hideLanded.value].filter(Boolean).length)
function clear(): void {
  typed.author = ''
  typed.message = ''
  typed.path = ''
  typed.changed = ''
  set({ from: null, not: null, author: null, message: null, path: null, changed: null, merges: null, landed: null })
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
      <USelectMenu v-model="branch" :items="branches" value-key="value" size="xs" class="w-48" :search-input="{ placeholder: t('pages.commits.findBranch') }" :aria-label="t('pages.commits.branch')" />
      <UBadge v-if="not" color="neutral" variant="subtle" size="md" class="gap-1">
        {{ t('pages.commits.notOn', { name: not }) }}
        <UButton icon="i-lucide-x" size="xs" color="neutral" variant="link" class="p-0" :aria-label="t('pages.commits.allOf', { name: from })" @click="set({ not: null })" />
      </UBadge>
      <UBadge v-if="message" color="neutral" variant="subtle" size="md" class="max-w-56 gap-1">
        <UIcon name="i-lucide-search" class="size-3 shrink-0" /><span class="truncate">{{ message }}</span>
        <UButton icon="i-lucide-x" size="xs" color="neutral" variant="link" class="p-0" :aria-label="t('pages.commits.clearMessage')" @click="typed.message = ''; set({ message: null })" />
      </UBadge>
      <!-- The rarer filters in one place, with how many are on: the row stays one line in a narrower window. -->
      <UPopover :content="{ align: 'start' }">
        <UButton :label="t('pages.commits.moreFilters')" icon="i-lucide-list-filter" size="xs" :color="moreOn ? 'primary' : 'neutral'" :variant="moreOn ? 'soft' : 'outline'">
          <template v-if="moreOn" #trailing><UBadge :label="String(moreOn)" size="sm" color="primary" variant="solid" /></template>
        </UButton>
        <template #content>
          <div class="grid w-72 gap-2 p-3">
            <UInput v-model="typed.author" icon="i-lucide-user" :placeholder="t('pages.commits.author')" size="sm" :aria-label="t('pages.commits.byAuthor')" />
            <UInput v-model="typed.path" icon="i-lucide-file" :placeholder="t('pages.commits.path')" size="sm" :ui="{ base: 'font-mono' }" :aria-label="t('pages.commits.byPath')" />
            <UInput v-model="typed.changed" icon="i-lucide-text-search" :placeholder="t('pages.commits.changed')" size="sm" :ui="{ base: 'font-mono' }" :aria-label="t('pages.commits.byChanged')" />
            <p class="text-label text-muted">{{ t('pages.commits.changedHelp') }}</p>
            <USelect v-model="mergeChoice" :items="MERGES" size="sm" :aria-label="t('pages.commits.merges')" />
            <USwitch v-model="hideLanded" size="sm" :label="t('pages.commits.hideLanded', { base: landed.base.value ?? 'main' })" :description="t('pages.commits.hideLandedHelp')" />
          </div>
        </template>
      </UPopover>
      <UButton v-if="filtered" :label="t('pages.commits.clear')" icon="i-lucide-x" size="xs" color="neutral" variant="ghost" @click="clear" />
      <UButton v-if="refs?.sha" label="HEAD" icon="i-lucide-crosshair" size="xs" color="neutral" variant="ghost" :title="t('pages.commits.pickHead')" @click="pick(refs.sha!, false)" />
    </div>
    <BranchApart v-if="apartName" :name="apartName" />
    <div ref="area" class="flex min-h-0 flex-1 flex-col">
      <CommitList :from="from" :not="not" :hide-landed="hideLanded" :author="author" :message="message" :path="path" :changed="changed" :merges="merges" :selected="selected" :compare="compare" class="min-h-0" :style="{ flex: selected ? `${1 - share} 1 0` : '1 1 0' }" @select="pick" />
      <!-- The details take room only once a commit is picked; until then the list has it all. -->
      <template v-if="selected">
        <div
          class="h-1.5 shrink-0 cursor-row-resize border-y border-default bg-elevated/50 hover:bg-accented"
          role="separator"
          aria-orientation="horizontal"
          :aria-label="t('pages.commits.resize')"
          @pointerdown.prevent="drag"
        />
        <div class="flex min-h-0 flex-col" :style="{ flex: `${share} 1 0` }">
          <CommitDetails :sha="selected" :compare="compare" class="min-h-0 flex-1" @select="sha => pick(sha, false)" />
        </div>
      </template>
      <p v-else class="shrink-0 border-t border-default px-3 py-1.5 text-label text-muted">{{ t('pages.commits.pick') }} {{ t('pages.commits.hint') }}</p>
    </div>
  </div>
</template>
