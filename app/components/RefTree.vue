<script setup lang="ts">
import type { BranchRef, RemoteBranchRef } from '~~/shared/types'

/*
 * Branch names as folders, split on "/": feature/login sits in a folder feature/. A folder with one branch only
 * still folds, so long lists stay short. Click one to list its commits, double-click to switch to it; a right click
 * gives the rest (useRefActions): a new branch from it, rename and delete, or for a remote branch, checking it out.
 */
type Leaf = BranchRef | RemoteBranchRef
interface Folder { name: string, path: string, folders: Folder[], leaves: { label: string, ref: Leaf }[] }

const props = defineProps<{ refs: Leaf[], storageKey: string, depth?: number, folder?: Folder, prefix?: string, expand?: boolean }>()
// A local branch's pull request on its host, when Settings has the host's tool switched on (useHostPulls).
const hostPulls = useHostPulls()
const CHECKS: Record<string, string> = { passing: 'text-success', failing: 'text-error', pending: 'text-warning' }
// While the sidebar is filtered every folder is open, so what matches shows.
const isOpen = (path: string) => !!props.expand || !!open.value[path]
const route = useRoute()
/** The name git knows it by: a remote branch with its remote in front. */
const full = (ref: Leaf) => `${props.prefix ?? ''}${ref.name}`

function build(refs: Leaf[]): Folder {
  const top: Folder = { name: '', path: '', folders: [], leaves: [] }
  for (const ref of refs) {
    const parts = ref.name.split('/')
    let at = top
    for (const part of parts.slice(0, -1)) {
      let next = at.folders.find(folder => folder.name === part)
      if (!next) at.folders.push(next = { name: part, path: `${at.path}${part}/`, folders: [], leaves: [] })
      at = next
    }
    at.leaves.push({ label: parts.at(-1) ?? ref.name, ref })
  }
  const sort = (folder: Folder) => {
    folder.folders.sort((a, b) => a.name.localeCompare(b.name))
    folder.leaves.sort((a, b) => a.label.localeCompare(b.label))
    folder.folders.forEach(sort)
  }
  sort(top)
  return top
}

const tree = computed(() => props.folder ?? build(props.refs))
const depth = computed(() => props.depth ?? 0)

const open = usePersisted<Record<string, boolean>>(`folders:${props.storageKey}`, {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
function toggle(path: string): void {
  open.value = { ...open.value, [path]: !open.value[path] }
}

const isBranch = (ref: Leaf): ref is BranchRef => 'current' in ref
const actions = useRefActions()
const undo = useOperation()
const webs = useWeb()
const { current } = useRefs()
const into = () => current.value?.name ?? t('RefTree.currentBranch')
const drag = useRefDrag()
const network = useNetwork()
const worktrees = useWorktrees()
const isCurrent = (ref: Leaf) => isBranch(ref) && ref.current
const landed = useLanded()
/** A local branch the main line has in full (squash-merged or rebased in), though git does not count it as merged. */
const inBase = (ref: Leaf) => isBranch(ref) && !ref.current && landed.whole.value.some(one => one.reference === `refs/heads/${ref.name}`)
// Branch cleanup, opened from a gone or landed branch (AppSidebar has it).
const cleanUp = inject<(() => void) | null>('branchCleanup', null)
// The sidebar's picked branches (local ones only; remote branches are not picked).
const pick = props.prefix === undefined ? inject<ReturnType<typeof useMultiSelect<BranchRef>> | null>('branchPick', null) : null
const picked = (ref: Leaf) => !!pick?.several.value && isBranch(ref) && pick.isPicked(ref)
function clickLeaf(ref: Leaf, event: MouseEvent): void {
  if (pick && isBranch(ref) && pick.click(ref, event)) event.preventDefault()
}

function menu(ref: Leaf) {
  const name = full(ref)
  if (pick && isBranch(ref) && pick.covering(ref).length > 1) {
    const names = pick.covering(ref).filter(one => !one.current).map(one => one.name)
    return [[{ label: t('RefTree.deleteSeveral', { count: names.length }), icon: 'i-lucide-trash-2', color: 'error' as const, disabled: !names.length, onSelect: () => actions.deleteBranches(names) }]]
  }
  const from = { rev: name, label: name }
  if (!isBranch(ref)) {
    return [[
      { label: t('RefTree.checkOut'), icon: 'i-lucide-git-branch', onSelect: () => actions.switchTo(name) },
      { label: t('RefTree.newBranch'), icon: 'i-lucide-git-branch-plus', onSelect: () => actions.newBranch(from) },
      { label: t('RefTree.merge', { name: into() }), icon: 'i-lucide-git-merge', onSelect: () => undo.merge(name) },
      // A remote branch's name on the remote is its name without the remote's.
      ...webs.label.value && props.prefix ? [{ label: webs.label.value, icon: 'i-lucide-external-link', onSelect: () => webs.openBranch(name.slice(props.prefix!.length)) }] : [],
      ...webs.copyLabel.value && props.prefix ? [{ label: webs.copyLabel.value, icon: 'i-lucide-link', onSelect: () => webs.copyBranch(name.slice(props.prefix!.length)) }] : [],
      { label: t('RefTree.copyName'), icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(name) }
    ]]
  }
  const remoteName = ref.upstream?.split('/').slice(1).join('/')
  const here = current.value
  return [
    [
      ...ref.current ? [] : [{ label: t('RefTree.switchTo'), icon: 'i-lucide-git-branch', onSelect: () => actions.switchTo(name) }],
      { label: t('RefTree.newBranch'), icon: 'i-lucide-git-branch-plus', onSelect: () => actions.newBranch(from) },
      { label: t('RefTree.newTag'), icon: 'i-lucide-tag', onSelect: () => actions.newTag(from) },
      ...ref.current ? [] : [{ label: t('RefTree.worktree'), icon: 'i-lucide-git-fork', onSelect: () => worktrees.add(name) }],
      ...ref.current ? [] : [{ label: t('RefTree.merge', { name: into() }), icon: 'i-lucide-git-merge', onSelect: () => undo.merge(name) }],
      // A branch you are not on: what differs from yours, and bringing it level with its remote branch either way.
      ...!ref.current && here ? [{ label: t('RefTree.compare', { name: here.name }), icon: 'i-lucide-git-compare', onSelect: () => navigateTo({ path: '/commits', query: { commit: ref.sha, compare: here.sha } }) }] : [],
      ...!ref.current && ref.upstream && !ref.gone && ref.behind ? [{ label: t('RefTree.fastForward', { upstream: ref.upstream, count: ref.behind }), icon: 'i-lucide-arrow-down-to-line', disabled: !!network.running.value, onSelect: () => network.fastForward(name) }] : [],
      ...!ref.current && (ref.ahead || !ref.upstream || ref.gone) ? [{ label: t('RefTree.pushBranch', { name }), icon: 'i-lucide-arrow-up-from-line', disabled: !!network.running.value, onSelect: () => network.pushBranch(name) }] : [],
      ...webs.label.value && remoteName ? [{ label: webs.label.value, icon: 'i-lucide-external-link', onSelect: () => webs.openBranch(remoteName) }] : [],
      ...webs.copyLabel.value && remoteName ? [{ label: webs.copyLabel.value, icon: 'i-lucide-link', onSelect: () => webs.copyBranch(remoteName) }] : []
    ],
    [
      { label: t('RefTree.copyName'), icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(name) },
      { label: t('RefTree.rename'), icon: 'i-lucide-pencil', onSelect: () => actions.renameBranch(name) },
      ...ref.current ? [] : [{ label: t('RefTree.delete'), icon: 'i-lucide-trash-2', color: 'error' as const, onSelect: () => actions.deleteBranch(name) }],
      ...cleanUp && (ref.gone || inBase(ref)) ? [{ label: t('RefTree.cleanUp'), icon: 'i-lucide-eraser', onSelect: cleanUp }] : []
    ],
    // Far apart from its remote branch: keep a copy and take the remote's.
    ...wentApart(ref) ? [[{ label: t('RefTree.matchUpstream', { upstream: ref.upstream! }), icon: 'i-lucide-copy', onSelect: () => actions.matchUpstream(ref) }]] : []
  ]
}
/** The remote branch's state, as one mark after the name: apart, gone, ahead and behind, in step, or none tracked. */
function remoteState(ref: BranchRef): { icon?: string, text?: string, class: string, title: string } | null {
  if (ref.gone) return { text: t('RefTree.gone'), class: 'text-warning', title: t('RefTree.goneHelp', { upstream: ref.upstream ?? '' }) }
  if (!ref.upstream) return { icon: 'i-lucide-cloud-off', class: 'text-dimmed', title: t('RefTree.localOnly') }
  if (wentApart(ref)) return { icon: 'i-lucide-triangle-alert', class: 'text-warning', title: t('RefTree.apart', { upstream: ref.upstream, ahead: ref.ahead, behind: ref.behind }) }
  if (!ref.ahead && !ref.behind) return { icon: 'i-lucide-cloud', class: 'text-dimmed', title: t('RefTree.inStep', { upstream: ref.upstream }) }
  return null
}
function switchTo(ref: Leaf): void {
  if (!isBranch(ref) || !ref.current) void actions.switchTo(full(ref))
}
function tip(ref: Leaf): string {
  const when = new Date(ref.time * 1000).toLocaleDateString(locale())
  if (!isBranch(ref)) return `${ref.subject} · ${when}`
  const tracking = ref.gone ? t('RefTree.upstreamGone', { upstream: ref.upstream ?? '' }) : ref.upstream ? t('RefTree.tracks', { upstream: ref.upstream }) : t('RefTree.tracksNothing')
  return `${ref.subject} · ${when} · ${tracking}`
}
</script>

<template>
  <ul class="grid grid-cols-[minmax(0,1fr)]">
    <li v-for="sub in tree.folders" :key="sub.path">
      <button type="button" class="flex w-full items-center gap-1 rounded py-1 pr-2 text-left text-ui text-muted hover:bg-elevated" :style="{ paddingLeft: `${0.5 + depth * 0.75}rem` }" :aria-expanded="isOpen(sub.path)" @click="toggle(sub.path)">
        <UIcon name="i-lucide-chevron-right" class="size-3 shrink-0 transition" :class="{ 'rotate-90': isOpen(sub.path) }" />
        <UIcon name="i-lucide-folder" class="size-3.5 shrink-0" />
        <span class="min-w-0 truncate">{{ sub.name }}</span>
      </button>
      <RefTree v-if="isOpen(sub.path)" :refs="[]" :folder="sub" :storage-key="props.storageKey" :prefix="props.prefix" :depth="depth + 1" :expand="props.expand" />
    </li>
    <li v-for="leaf in tree.leaves" :key="leaf.ref.name">
      <UContextMenu :items="menu(leaf.ref)">
      <NuxtLink
        :to="{ path: '/commits', query: { from: full(leaf.ref) } }"
        class="flex items-center gap-1.5 rounded py-1 pr-2 text-ui hover:bg-elevated"
        :class="[
          { 'bg-elevated': pick?.several.value ? picked(leaf.ref) : route.path === '/commits' && route.query.from === full(leaf.ref), 'text-highlighted': picked(leaf.ref) },
          drag.over.value === full(leaf.ref) && drag.hint(full(leaf.ref), isCurrent(leaf.ref)) ? 'bg-primary/15 ring-1 ring-primary' : '',
          drag.dragged.value?.name === full(leaf.ref) ? 'opacity-50' : ''
        ]"
        :style="{ paddingLeft: `${1.25 + depth * 0.75}rem` }"
        :title="drag.dragged.value ? drag.hint(full(leaf.ref), isCurrent(leaf.ref)) ?? undefined : tip(leaf.ref)"
        :data-ref="full(leaf.ref)"
        :data-current="isCurrent(leaf.ref)"
        draggable="false"
        @pointerdown="drag.down($event, full(leaf.ref), isCurrent(leaf.ref))"
        @click.capture="drag.endedDrag($event)"
        @click="clickLeaf(leaf.ref, $event)"
        @dblclick.prevent="switchTo(leaf.ref)"
      >
        <UIcon v-if="isBranch(leaf.ref) && leaf.ref.current" name="i-lucide-check" class="size-3.5 shrink-0 text-highlighted" />
        <UIcon v-else name="i-lucide-git-branch" class="size-3.5 shrink-0 text-dimmed" />
        <span class="min-w-0 flex-1 truncate" :class="isBranch(leaf.ref) && leaf.ref.current ? 'font-semibold text-highlighted' : ''">{{ leaf.label }}</span>
        <span v-if="drag.over.value === full(leaf.ref) && drag.hint(full(leaf.ref), isCurrent(leaf.ref))" class="shrink-0 text-label text-primary">{{ isCurrent(leaf.ref) ? t('RefTree.mergeHere') : t('RefTree.rebaseOnto') }}</span>
        <template v-if="isBranch(leaf.ref)">
          <UIcon v-if="inBase(leaf.ref)" name="i-lucide-check-check" class="size-3.5 shrink-0 text-success" :title="t('RefTree.inBase', { base: landed.base.value ?? 'main' })" />
          <template v-for="state in [remoteState(leaf.ref)]" :key="state?.title ?? 'none'">
            <button v-if="state?.text && cleanUp" type="button" class="shrink-0 text-label hover:underline" :class="state.class" :title="state.title" @click.stop.prevent="cleanUp()">{{ state.text }}</button>
            <span v-else-if="state?.text" class="shrink-0 text-label" :class="state.class" :title="state.title">{{ state.text }}</span>
            <UIcon v-else-if="state?.icon" :name="state.icon" class="size-3 shrink-0" :class="state.class" :title="state.title" />
          </template>
          <span v-if="leaf.ref.ahead" class="font-mono text-label text-muted tabular-nums" :title="t('RefTree.toPush', { count: leaf.ref.ahead })">↑{{ leaf.ref.ahead }}</span>
          <span v-if="leaf.ref.behind" class="font-mono text-label text-muted tabular-nums" :title="t('RefTree.toPull', { count: leaf.ref.behind })">↓{{ leaf.ref.behind }}</span>
          <button
            v-if="hostPulls.byBranch.value.get(leaf.ref.name)"
            type="button"
            class="flex items-center gap-0.5 font-mono text-label tabular-nums hover:underline"
            :class="['open', 'draft'].includes(hostPulls.byBranch.value.get(leaf.ref.name)!.state) ? CHECKS[hostPulls.byBranch.value.get(leaf.ref.name)!.checks ?? ''] ?? 'text-muted' : 'text-dimmed'"
            :title="`${t('RefTree.pullTitle', { pull: describePull(hostPulls.byBranch.value.get(leaf.ref.name)!) })}${hostPulls.untested.value}`"
            @click.stop.prevent="hostPulls.openPull(hostPulls.byBranch.value.get(leaf.ref.name)!)"
          >
            <UIcon name="i-lucide-git-pull-request" class="size-3" />{{ hostPulls.byBranch.value.get(leaf.ref.name)!.number }}
          </button>
        </template>
      </NuxtLink>
      </UContextMenu>
    </li>
  </ul>
</template>
