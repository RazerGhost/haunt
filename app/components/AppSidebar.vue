<script setup lang="ts">
/*
 * The sidebar. In Git: the local changes and the commits, the repository's branches, its worktrees
 * (WorktreeList), remotes, tags and stashes. Click a branch, remote branch or tag to list its commits, a stash to see what it saved; a right click acts on
 * it (useRefActions). In Code, the views of the code and the project's parts instead (CodeSidebar); one width for both.
 */
const route = useRoute()
const { refs, localChanges, notRepository } = useRefs()
const actions = useRefActions()
const network = useNetwork()
const { repo, gitChanged } = useRepositories()
// The dropped stashes that can come back, read with the stashes.
watch([repo, gitChanged('stash')], () => void actions.loadDropped(), { immediate: true })
const when = (seconds: number) => new Date(seconds * 1000).toLocaleString()

const shut = usePersisted<Record<string, boolean>>('sidebar-shut', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
/** The Git screens without a place of their own in the sidebar. */
const tools = [[
  { label: 'Bisect', icon: 'i-lucide-crosshair', to: '/bisect' },
  { label: 'Release notes', icon: 'i-lucide-scroll-text', to: '/release' },
  { label: 'Apply patches', icon: 'i-lucide-file-diff', to: '/patches' },
  { label: 'Health', icon: 'i-lucide-heart-pulse', to: '/health' }
]]
function toggle(section: string): void {
  shut.value = { ...shut.value, [section]: !shut.value[section] }
}

const { mode } = useMode()
const cleaning = ref(false)
const remoteBranches = computed(() => shown.value?.remotes.reduce((sum, remote) => sum + remote.branches.length, 0) ?? 0)

// The filter above the lists: branches, remote branches, tags and stashes whose name has the text (any case). It shows
// once there are more than a few, and is kept per repository while Haunt runs.
const filter = ref('')
watch(repo, () => { filter.value = '' })
const refCount = computed(() => {
  const all = refs.value
  return all ? all.branches.length + all.tags.length + all.stashes.length + all.remotes.reduce((sum, remote) => sum + remote.branches.length, 0) : 0
})
const shown = computed(() => {
  const all = refs.value
  const text = filter.value.trim().toLowerCase()
  if (!all || !text) return all
  const has = (name: string) => name.toLowerCase().includes(text)
  return {
    ...all,
    branches: all.branches.filter(item => has(item.name)),
    remotes: all.remotes.map(remote => ({ ...remote, branches: remote.branches.filter(item => has(`${remote.name}/${item.name}`)) })).filter(remote => remote.branches.length),
    tags: all.tags.filter(item => has(item.name)),
    stashes: all.stashes.filter(item => has(item.message))
  }
})

// Its width, dragged by its right edge (a double-click puts it back), kept between launches. Names longer than the
// width end in an ellipsis; the sidebar never scrolls sideways.
const WIDTH = 240
const width = usePersisted('sidebar-width', WIDTH, value => typeof value === 'number' && value >= 180 && value <= 480)
function drag(start: PointerEvent): void {
  const handle = start.target as HTMLElement
  const from = width.value - start.clientX
  handle.setPointerCapture(start.pointerId)
  const moveTo = (event: PointerEvent) => { width.value = Math.min(480, Math.max(180, Math.round(from + event.clientX))) }
  const stop = () => {
    handle.removeEventListener('pointermove', moveTo)
    handle.removeEventListener('pointerup', stop)
  }
  handle.addEventListener('pointermove', moveTo)
  handle.addEventListener('pointerup', stop)
}

const item = 'flex items-center gap-2 rounded-md px-2 py-1 text-ui'
const active = (path: string) => route.path === path ? 'bg-elevated font-medium text-highlighted' : 'text-default hover:bg-elevated/60'
</script>

<template>
  <div class="relative flex shrink-0">
  <nav class="flex shrink-0 flex-col gap-3 overflow-x-hidden overflow-y-auto border-r border-default bg-elevated/20 px-2 py-3 select-none" :style="{ width: `${width}px` }" :aria-label="mode === 'code' ? 'Code' : 'Repository'">
    <CodeSidebar v-if="mode === 'code'" />
    <template v-else>
    <div class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
      <NuxtLink to="/local" :class="[item, active('/local')]">
        <UIcon name="i-lucide-pencil-line" class="size-4 shrink-0 text-muted" />
        <span class="flex-1">Local changes</span>
        <span v-if="localChanges" class="rounded-full bg-warning/15 px-1.5 text-label font-semibold text-warning tabular-nums">{{ localChanges }}</span>
      </NuxtLink>
      <NuxtLink to="/commits" :class="[item, route.path === '/commits' && !route.query.from ? 'bg-elevated font-medium text-highlighted' : 'text-default hover:bg-elevated/60']">
        <UIcon name="i-lucide-git-commit-vertical" class="size-4 shrink-0 text-muted" />
        <span class="flex-1">All commits</span>
      </NuxtLink>
      <NuxtLink to="/pulls" :class="[item, active('/pulls')]">
        <UIcon name="i-lucide-git-pull-request" class="size-4 shrink-0 text-muted" />
        <span class="flex-1">Pull requests</span>
      </NuxtLink>
      <UDropdownMenu :items="tools" :content="{ align: 'start', side: 'right' }">
        <button type="button" :class="[item, tools[0]!.some(tool => route.path === tool.to) ? 'bg-elevated font-medium text-highlighted' : 'text-default hover:bg-elevated/60']">
          <UIcon name="i-lucide-wrench" class="size-4 shrink-0 text-muted" />
          <span class="flex-1 text-left">More</span>
          <UIcon name="i-lucide-chevron-right" class="size-3.5 text-dimmed" />
        </button>
      </UDropdownMenu>
    </div>

    <p v-if="notRepository" class="px-2 text-ui text-muted">Not a git repository.</p>

    <UInput v-if="refs && refCount > 6" v-model="filter" icon="i-lucide-filter" placeholder="Filter branches, tags, stashes" size="xs" class="px-1" aria-label="Filter branches, tags and stashes">
      <template v-if="filter" #trailing>
        <UButton icon="i-lucide-x" size="xs" color="neutral" variant="link" aria-label="Clear the filter" @click="filter = ''" />
      </template>
    </UInput>
    <template v-if="refs && shown">
      <section class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
        <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!shut.branches" @click="toggle('branches')">
          <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !shut.branches }" />
          Branches <span class="ml-auto font-normal tabular-nums">{{ shown.branches.length }}</span>
          <UTooltip text="Clean up merged branches and branches whose remote is gone">
            <span role="button" tabindex="0" class="-mr-1 grid size-5 place-items-center rounded normal-case hover:bg-elevated hover:text-highlighted" aria-label="Clean up branches" @click.stop="cleaning = true" @keydown.enter.stop="cleaning = true">
              <UIcon name="i-lucide-eraser" class="size-3.5" />
            </span>
          </UTooltip>
        </button>
        <RefTree v-if="!shut.branches" :refs="shown.branches" storage-key="branches" :expand="!!filter" />
      </section>

      <WorktreeList :shut="!!shut.worktrees" @toggle="toggle('worktrees')" />
      <SubmoduleList :shut="!!shut.submodules" @toggle="toggle('submodules')" />

      <section v-if="shown.remotes.length" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
        <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!shut.remotes" @click="toggle('remotes')">
          <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !shut.remotes }" />
          Remotes <span class="ml-auto font-normal tabular-nums">{{ remoteBranches }}</span>
        </button>
        <template v-if="!shut.remotes">
          <div v-for="remote in shown.remotes" :key="remote.name" class="grid grid-cols-[minmax(0,1fr)]">
            <div class="flex items-center gap-1.5 px-2 py-1 text-ui text-muted">
              <UIcon name="i-lucide-cloud" class="size-3.5" />{{ remote.name }}
            </div>
            <RefTree :refs="remote.branches" :storage-key="`remote:${remote.name}`" :prefix="`${remote.name}/`" :depth="1" :expand="!!filter" />
          </div>
        </template>
      </section>

      <section v-if="shown.tags.length" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
        <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!shut.tags" @click="toggle('tags')">
          <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !shut.tags }" />
          Tags <span class="ml-auto font-normal tabular-nums">{{ shown.tags.length }}</span>
        </button>
        <ul v-if="!shut.tags" class="grid grid-cols-[minmax(0,1fr)]">
          <li v-for="tag in shown.tags" :key="tag.name">
            <UContextMenu :items="[[{ label: 'New branch from here…', icon: 'i-lucide-git-branch-plus', onSelect: () => actions.newBranch({ rev: tag.name, label: tag.name }) }, { label: 'Push to the remote', icon: 'i-lucide-arrow-up-from-line', disabled: !network.trusted.value || !!network.running.value, onSelect: () => network.pushTag(tag.name) }, { label: 'Release notes up to here…', icon: 'i-lucide-scroll-text', onSelect: () => navigateTo({ path: '/release', query: { to: tag.name } }) }], [{ label: 'Delete…', icon: 'i-lucide-trash-2', color: 'error', onSelect: () => actions.deleteTag(tag.name) }]]">
            <NuxtLink
              :to="{ path: '/commits', query: { from: tag.name } }"
              class="flex items-center gap-1.5 rounded py-1 pr-2 pl-5 text-ui hover:bg-elevated"
              :class="{ 'bg-elevated text-highlighted': route.path === '/commits' && route.query.from === tag.name }"
              :title="`${tag.sha.slice(0, 7)} · ${new Date(tag.time * 1000).toLocaleDateString()}`"
            >
              <UIcon name="i-lucide-tag" class="size-3.5 shrink-0 text-dimmed" />
              <span class="min-w-0 truncate">{{ tag.name }}</span>
            </NuxtLink>
            </UContextMenu>
          </li>
        </ul>
      </section>

      <section v-if="shown.stashes.length || actions.dropped.value.length" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
        <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!shut.stashes" @click="toggle('stashes')">
          <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !shut.stashes }" />
          Stashes <span class="ml-auto font-normal tabular-nums">{{ shown.stashes.length }}</span>
        </button>
        <ul v-if="!shut.stashes" class="grid grid-cols-[minmax(0,1fr)]">
          <li v-for="stash in shown.stashes" :key="stash.sha">
            <UContextMenu :items="[[{ label: 'Apply', icon: 'i-lucide-archive-restore', onSelect: () => actions.applyStash(stash.index, false) }, { label: 'Pop (apply and drop)', icon: 'i-lucide-package-open', onSelect: () => actions.applyStash(stash.index, true) }], [{ label: 'Drop…', icon: 'i-lucide-trash-2', color: 'error', onSelect: () => actions.dropStash(stash.index, stash.message) }]]">
              <NuxtLink
                :to="{ path: '/commits', query: { commit: stash.sha } }"
                class="flex items-center gap-1.5 rounded py-1 pr-2 pl-5 text-ui hover:bg-elevated"
                :class="{ 'bg-elevated text-highlighted': route.path === '/commits' && route.query.commit === stash.sha }"
                :title="`${stash.message} · ${when(stash.time)} · right-click to apply, pop or drop`"
              >
                <UIcon name="i-lucide-archive" class="size-3.5 shrink-0 text-dimmed" />
                <span class="min-w-0 truncate">{{ stash.message }}</span>
              </NuxtLink>
            </UContextMenu>
          </li>
        </ul>
        <details v-if="!shut.stashes && actions.dropped.value.length" class="pr-2 pl-5 text-ui">
          <summary class="cursor-pointer py-1 text-muted">Recently dropped <span class="tabular-nums">{{ actions.dropped.value.length }}</span></summary>
          <ul class="grid grid-cols-[minmax(0,1fr)] gap-1">
            <li v-for="entry in actions.dropped.value" :key="entry.id" class="flex items-center gap-1.5">
              <span class="min-w-0 flex-1 truncate text-muted" :title="`${entry.message} · dropped ${when(entry.at)}`">{{ entry.message }}</span>
              <UButton label="Restore" size="xs" color="neutral" variant="ghost" @click="actions.restoreStash(entry.id)" />
            </li>
          </ul>
        </details>
      </section>
    </template>
    </template>
  </nav>
  <BranchCleanup v-model:open="cleaning" />
  <div
    class="absolute inset-y-0 -right-1 z-10 w-2 cursor-col-resize hover:bg-accented/60"
    role="separator"
    aria-orientation="vertical"
    aria-label="Drag to resize the sidebar"
    title="Drag to resize · double-click to reset"
    @pointerdown.prevent="drag"
    @dblclick="width = WIDTH"
  />
  </div>
</template>
