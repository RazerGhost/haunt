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
const into = () => current.value?.name ?? 'the current branch'
const drag = useRefDrag()
const worktrees = useWorktrees()
const isCurrent = (ref: Leaf) => isBranch(ref) && ref.current

function menu(ref: Leaf) {
  const name = full(ref)
  const from = { rev: name, label: name }
  if (!isBranch(ref)) {
    return [[
      { label: 'Check out', icon: 'i-lucide-git-branch', onSelect: () => actions.switchTo(name) },
      { label: 'New branch from here…', icon: 'i-lucide-git-branch-plus', onSelect: () => actions.newBranch(from) },
      { label: `Merge into ${into()}…`, icon: 'i-lucide-git-merge', onSelect: () => undo.merge(name) },
      // A remote branch's name on the remote is its name without the remote's.
      ...webs.label.value && props.prefix ? [{ label: webs.label.value, icon: 'i-lucide-external-link', onSelect: () => webs.openBranch(name.slice(props.prefix!.length)) }] : [],
      ...webs.copyLabel.value && props.prefix ? [{ label: webs.copyLabel.value, icon: 'i-lucide-link', onSelect: () => webs.copyBranch(name.slice(props.prefix!.length)) }] : [],
      { label: 'Copy the name', icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(name) }
    ]]
  }
  const remoteName = ref.upstream?.split('/').slice(1).join('/')
  return [
    [
      ...ref.current ? [] : [{ label: 'Switch to it', icon: 'i-lucide-git-branch', onSelect: () => actions.switchTo(name) }],
      { label: 'New branch from here…', icon: 'i-lucide-git-branch-plus', onSelect: () => actions.newBranch(from) },
      { label: 'New tag here…', icon: 'i-lucide-tag', onSelect: () => actions.newTag(from) },
      ...ref.current ? [] : [{ label: 'Open in worktree…', icon: 'i-lucide-git-fork', onSelect: () => worktrees.add(name) }],
      ...ref.current ? [] : [{ label: `Merge into ${into()}…`, icon: 'i-lucide-git-merge', onSelect: () => undo.merge(name) }],
      ...webs.label.value && remoteName ? [{ label: webs.label.value, icon: 'i-lucide-external-link', onSelect: () => webs.openBranch(remoteName) }] : [],
      ...webs.copyLabel.value && remoteName ? [{ label: webs.copyLabel.value, icon: 'i-lucide-link', onSelect: () => webs.copyBranch(remoteName) }] : []
    ],
    [
      { label: 'Copy the name', icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(name) },
      { label: 'Rename…', icon: 'i-lucide-pencil', onSelect: () => actions.renameBranch(name) },
      ...ref.current ? [] : [{ label: 'Delete…', icon: 'i-lucide-trash-2', color: 'error' as const, onSelect: () => actions.deleteBranch(name) }]
    ]
  ]
}
function switchTo(ref: Leaf): void {
  if (!isBranch(ref) || !ref.current) void actions.switchTo(full(ref))
}
function tip(ref: Leaf): string {
  const when = new Date(ref.time * 1000).toLocaleDateString()
  if (!isBranch(ref)) return `${ref.subject} · ${when}`
  const tracking = ref.gone ? `tracked ${ref.upstream}, which is gone` : ref.upstream ? `tracks ${ref.upstream}` : 'tracks no remote branch'
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
          { 'bg-elevated': route.path === '/commits' && route.query.from === full(leaf.ref) },
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
        @dblclick.prevent="switchTo(leaf.ref)"
      >
        <UIcon v-if="isBranch(leaf.ref) && leaf.ref.current" name="i-lucide-check" class="size-3.5 shrink-0 text-highlighted" />
        <UIcon v-else name="i-lucide-git-branch" class="size-3.5 shrink-0 text-dimmed" />
        <span class="min-w-0 flex-1 truncate" :class="isBranch(leaf.ref) && leaf.ref.current ? 'font-semibold text-highlighted' : ''">{{ leaf.label }}</span>
        <span v-if="drag.over.value === full(leaf.ref) && drag.hint(full(leaf.ref), isCurrent(leaf.ref))" class="shrink-0 text-label text-primary">{{ isCurrent(leaf.ref) ? 'merge here' : 'rebase onto' }}</span>
        <template v-if="isBranch(leaf.ref)">
          <span v-if="leaf.ref.gone" class="text-label text-warning">gone</span>
          <span v-if="leaf.ref.ahead" class="font-mono text-label text-muted tabular-nums" :title="`${leaf.ref.ahead} to push`">↑{{ leaf.ref.ahead }}</span>
          <span v-if="leaf.ref.behind" class="font-mono text-label text-muted tabular-nums" :title="`${leaf.ref.behind} to pull`">↓{{ leaf.ref.behind }}</span>
          <button
            v-if="hostPulls.byBranch.value.get(leaf.ref.name)"
            type="button"
            class="flex items-center gap-0.5 font-mono text-label tabular-nums hover:underline"
            :class="['open', 'draft'].includes(hostPulls.byBranch.value.get(leaf.ref.name)!.state) ? CHECKS[hostPulls.byBranch.value.get(leaf.ref.name)!.checks ?? ''] ?? 'text-muted' : 'text-dimmed'"
            :title="`${describePull(hostPulls.byBranch.value.get(leaf.ref.name)!)}. Click to open it on the web.${hostPulls.untested.value}`"
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
