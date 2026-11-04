<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
import type { LineRef, LocalFile } from '~/bindings'

/*
 * Local changes as files: the unstaged files above, the staged ones below, the picked file's diff on the
 * right (lines can be picked in it) and the commit box under it. Double-click a file to stage or unstage it; from the
 * keyboard, ↑ and ↓ move through the files and Space stages or unstages the picked one. Ctrl-click (⌘ on macOS) adds
 * or takes a file out of the selection and Shift-click (or Shift ↑ ↓) selects a run of them, in one list at a time;
 * staging, unstaging, discarding and stashing a selected file then do all the selected ones; Ctrl A selects a whole
 * list (app/plugins/select-all.client.ts). A discard asks first and is
 * saved: "Recently discarded" brings it back for 30 days. With nothing changed, the right shows the clean working tree
 * instead (LocalClean: the branch against its upstream, the last commit to amend, the stashes), without the commit box
 * until Amend opens it. Right-click a file to ignore it (an untracked one: the file,
 * its extension or its folder, added to .gitignore) or to stash just it; Stash… stashes the files ticked.
 */
const { status, discarded, problem, busy, stage, unstage, lines, discard, restoreAll, ignore, stashFiles } = useLocalChanges()
// Several of Recently discarded, picked to bring back together.
const discardedPick = useMultiSelect(() => discarded.value, entry => entry.id, 'discarded')

const picked = ref<{ path: string, staged: boolean } | null>(null)
const unstaged = computed(() => status.value?.unstaged ?? [])
const staged = computed(() => status.value?.staged ?? [])
const clean = computed(() => !!status.value && !unstaged.value.length && !staged.value.length)
// Amend, from the clean view: the commit box opens in amend mode until committed or unticked.
const amending = ref(false)
const current = computed<LocalFile | null>(() => {
  if (!picked.value) return null
  return (picked.value.staged ? staged.value : unstaged.value).find(file => file.path === picked.value!.path) ?? null
})
// When the picked file leaves its list (staged, discarded, committed), the first file left is shown.
watch(status, () => {
  if (current.value) return
  const next = unstaged.value[0] ?? staged.value[0]
  picked.value = next ? { path: next.path, staged: !unstaged.value[0] } : null
}, { immediate: true })

/* The selection: files of one list, the picked one among them. Shift selects from the anchor, the file last clicked
 * without Shift. */
const selection = ref<{ staged: boolean, paths: Set<string> }>({ staged: false, paths: new Set() })
let anchor: { path: string, staged: boolean } | null = null
const listOf = (isStaged: boolean) => (isStaged ? staged.value : unstaged.value)
const isSelected = (path: string, isStaged: boolean) => selection.value.staged === isStaged && selection.value.paths.has(path)
// What an action on `path` covers: the whole selection when the file is in it, else the file alone.
function covered(path: string, isStaged: boolean): string[] {
  return isSelected(path, isStaged) && selection.value.paths.size > 1 ? listOf(isStaged).map(file => file.path).filter(known => selection.value.paths.has(known)) : [path]
}
function select(path: string, isStaged: boolean, how: { ctrl?: boolean, shift?: boolean } = {}): void {
  const sameList = selection.value.staged === isStaged
  const from = anchor && anchor.staged === isStaged ? anchor.path : path
  let paths: Set<string>
  if (how.shift) {
    const list = listOf(isStaged).map(file => file.path)
    const [a, b] = [list.indexOf(from), list.indexOf(path)].sort((x, y) => x - y)
    paths = new Set([...how.ctrl && sameList ? selection.value.paths : [], ...list.slice(Math.max(0, a!), b! + 1)])
  } else if (how.ctrl) {
    paths = new Set(sameList ? selection.value.paths : [])
    if (paths.has(path) && paths.size > 1) paths.delete(path)
    else paths.add(path)
    anchor = { path, staged: isStaged }
  } else {
    paths = new Set([path])
    anchor = { path, staged: isStaged }
  }
  selection.value = { staged: isStaged, paths }
  picked.value = { path, staged: isStaged }
}
function onClick(event: MouseEvent, path: string, isStaged: boolean): void {
  select(path, isStaged, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey })
}
// The selection follows the picked file, and keeps only files still in its list.
watch(picked, (now) => {
  if (now && !isSelected(now.path, now.staged)) {
    selection.value = { staged: now.staged, paths: new Set([now.path]) }
    anchor = { ...now }
  }
})
watch(status, () => {
  const left = new Set(listOf(selection.value.staged).map(file => file.path))
  const kept = [...selection.value.paths].filter(path => left.has(path))
  if (kept.length !== selection.value.paths.size) selection.value = { staged: selection.value.staged, paths: new Set(kept) }
  if (picked.value && !isSelected(picked.value.path, picked.value.staged)) selection.value = { staged: picked.value.staged, paths: new Set([picked.value.path]) }
})
const stageCovered = (path: string, isStaged: boolean) => (isStaged ? unstage(covered(path, true)) : stage(covered(path, false)))
// Ctrl A in a list: all of its files, the picked one kept when it is in it.
function selectAll(event: Event): void {
  const name = (event as CustomEvent<string>).detail
  if (name !== 'unstaged' && name !== 'staged') return
  const isStaged = name === 'staged'
  const list = listOf(isStaged)
  if (!list.length) return
  const keep = picked.value?.staged === isStaged && list.some(file => file.path === picked.value!.path)
  selection.value = { staged: isStaged, paths: new Set(list.map(file => file.path)) }
  if (!keep) picked.value = { path: list[0]!.path, staged: isStaged }
}
onMounted(() => window.addEventListener('haunt:select-all', selectAll))
onBeforeUnmount(() => window.removeEventListener('haunt:select-all', selectAll))

// ↑ ↓ move through both lists (with Shift, the selection grows within its list), Space stages or unstages the
// selected files; not while typing (the commit box).
function onKey(event: KeyboardEvent): void {
  const target = event.target as HTMLElement | null
  if (event.ctrlKey || event.metaKey || event.altKey || !target || target.closest('input, textarea, select, [contenteditable], [role=dialog]')) return
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    const step = event.key === 'ArrowDown' ? 1 : -1
    if (event.shiftKey && picked.value) {
      const list = listOf(picked.value.staged)
      const at = list.findIndex(file => file.path === picked.value!.path)
      const next = list[Math.min(list.length - 1, Math.max(0, at + step))]
      if (next) select(next.path, picked.value.staged, { shift: true })
    } else {
      const all = [...unstaged.value.map(file => ({ path: file.path, staged: false })), ...staged.value.map(file => ({ path: file.path, staged: true }))]
      const at = all.findIndex(item => item.path === picked.value?.path && item.staged === picked.value?.staged)
      const next = all[Math.min(all.length - 1, Math.max(0, at + step))]
      if (next) select(next.path, next.staged)
    }
    event.preventDefault()
  } else if (event.key === ' ' && current.value && picked.value && !busy.value) {
    void stageCovered(current.value.path, picked.value.staged)
    event.preventDefault()
  }
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))

const STATUS = {
  added: { letter: 'A', class: 'text-success' },
  modified: { letter: 'M', class: 'text-warning' },
  deleted: { letter: 'D', class: 'text-error' },
  renamed: { letter: 'R', class: 'text-info' },
  copied: { letter: 'C', class: 'text-info' },
  type: { letter: 'T', class: 'text-muted' }
} as const
const folder = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : '')
const name = (path: string) => path.slice(path.lastIndexOf('/') + 1)

/* Discarding asks first */

const confirming = ref<{ paths: string[] } | { path: string, lines: LineRef[] } | null>(null)
const open = computed({ get: () => confirming.value !== null, set: (value) => { if (!value) confirming.value = null } })
const diffView = useTemplateRef<{ clearPicks: () => void }>('diffView')
async function confirmDiscard(): Promise<void> {
  const what = confirming.value
  confirming.value = null
  if (!what) return
  if ('paths' in what) await discard(what.paths)
  else if (await lines(what.path, what.lines, 'discard')) diffView.value?.clearPicks()
}
function askDiscard(what: { lines: LineRef[] } | { file: true }): void {
  if (!current.value) return
  confirming.value = 'lines' in what ? { path: current.value.path, lines: what.lines } : { paths: covered(current.value.path, false) }
}
const confirmText = computed(() => {
  const what = confirming.value
  if (!what) return ''
  if ('lines' in what) return t('LocalChanges.pickedLines', { count: what.lines.length, path: what.path })
  return what.paths.length === 1 ? what.paths[0]! : t('LocalChanges.files', { count: what.paths.length })
})
const when = (seconds: number) => new Date(seconds * 1000).toLocaleString(locale())

/* A file's menu: stage or unstage, stash just it, and for an untracked file, ignore it, its extension or its folder.
 * On a selected file of several, the menu is for all the selected ones (ignoring stays one file's). */
function fileMenu(file: LocalFile, isStaged: boolean) {
  const paths = covered(file.path, isStaged)
  if (paths.length > 1) {
    const untracked = listOf(isStaged).some(known => known.untracked && paths.includes(known.path))
    const count = paths.length
    return [
      [
        isStaged
          ? { label: t('LocalChanges.unstageSelected', { count }), icon: 'i-lucide-minus', onSelect: () => unstage(paths) }
          : { label: t('LocalChanges.stageSelected', { count }), icon: 'i-lucide-plus', onSelect: () => stage(paths) },
        { label: t('LocalChanges.stashSelected', { count }), icon: 'i-lucide-archive', onSelect: () => stashFiles(paths, untracked, '') }
      ],
      ...isStaged ? [] : [[{ label: t('LocalChanges.discardSelected', { count }), icon: 'i-lucide-undo-2', color: 'error' as const, onSelect: () => { confirming.value = { paths } } }]]
    ] satisfies ContextMenuItem[][]
  }
  const items: ContextMenuItem[][] = [[
    isStaged
      ? { label: t('LocalChanges.unstage'), icon: 'i-lucide-minus', onSelect: () => unstage([file.path]) }
      : { label: t('LocalChanges.stage'), icon: 'i-lucide-plus', onSelect: () => stage([file.path]) },
    { label: t('LocalChanges.stashFile'), icon: 'i-lucide-archive', onSelect: () => stashFiles([file.path], file.untracked, '') }
  ]]
  if (!isStaged && file.untracked) {
    const extension = name(file.path).includes('.') && !name(file.path).startsWith('.') ? name(file.path).slice(name(file.path).lastIndexOf('.')) : ''
    const dir = folder(file.path)
    items.push([
      { label: t('LocalChanges.ignoreFile'), icon: 'i-lucide-eye-off', onSelect: () => ignore([`/${file.path}`]) },
      ...extension ? [{ label: t('LocalChanges.ignoreExtension', { extension }), icon: 'i-lucide-eye-off', onSelect: () => ignore([`*${extension}`]) }] : [],
      ...dir ? [{ label: t('LocalChanges.ignoreFolder', { folder: dir }), icon: 'i-lucide-eye-off', onSelect: () => ignore([`/${dir}`]) }] : []
    ])
  }
  if (!isStaged) items.push([{ label: t('LocalChanges.discard'), icon: 'i-lucide-undo-2', color: 'error' as const, onSelect: () => { confirming.value = { paths: [file.path] } } }])
  return items
}

/* Stash…: the changed files with a tick each (all ticked to start), and a message. */
const stashing = ref(false)
const stashTicked = ref<Set<string>>(new Set())
const stashMessage = ref('')
const changedFiles = computed(() => {
  const seen = new Map<string, LocalFile>()
  for (const file of [...staged.value, ...unstaged.value]) if (!seen.has(file.path)) seen.set(file.path, file)
  return [...seen.values()]
})
function openStash(): void {
  stashTicked.value = new Set(changedFiles.value.map(file => file.path))
  stashMessage.value = ''
  stashing.value = true
}
function tick(path: string, on: boolean): void {
  const next = new Set(stashTicked.value)
  if (on) next.add(path)
  else next.delete(path)
  stashTicked.value = next
}
async function confirmStash(): Promise<void> {
  const paths = [...stashTicked.value]
  if (!paths.length) return
  const untracked = changedFiles.value.some(file => file.untracked && stashTicked.value.has(file.path))
  stashing.value = false
  await stashFiles(paths, untracked, stashMessage.value)
}
</script>

<template>
  <div class="grid min-h-0 flex-1 grid-cols-[20rem_minmax(0,1fr)]">
    <aside class="flex min-h-0 flex-col border-r border-default select-none">
      <section class="flex min-h-0 flex-col" :class="clean ? 'flex-none' : 'flex-1'">
        <header class="flex items-center gap-1 border-b border-default px-3 py-1.5">
          <h2 class="flex-1 text-label font-semibold tracking-wider text-muted uppercase">{{ t('LocalChanges.unstaged') }} <span class="font-normal tabular-nums">{{ unstaged.length }}</span></h2>
          <UButton v-if="unstaged.length || staged.length" icon="i-lucide-archive" size="xs" color="neutral" variant="ghost" :aria-label="t('LocalChanges.stashSome')" :title="t('LocalChanges.stashSome')" :disabled="busy" @click="openStash" />
          <UButton v-if="!selection.staged && selection.paths.size > 1" :label="t('LocalChanges.stageSelected', { count: selection.paths.size })" size="xs" color="neutral" variant="soft" :disabled="busy" @click="stage(covered(picked!.path, false))" />
          <UButton v-else-if="unstaged.length" :label="t('LocalChanges.stageAll')" size="xs" color="neutral" variant="ghost" :disabled="busy" @click="stage(unstaged.map(file => file.path))" />
          <UButton v-if="unstaged.length" icon="i-lucide-undo-2" size="xs" color="neutral" variant="ghost" :aria-label="t('LocalChanges.discardAll')" :title="t('LocalChanges.discardAll')" :disabled="busy" @click="confirming = { paths: unstaged.map(file => file.path) }" />
        </header>
        <ul data-select-all="unstaged" class="min-h-0 flex-1 overflow-y-auto" :class="clean ? 'py-0.5' : 'py-1'">
          <UContextMenu v-for="file in unstaged" :key="file.path" :items="fileMenu(file, false)">
            <li
              class="group flex cursor-default items-center gap-2 px-3 py-1 text-ui"
              :class="isSelected(file.path, false) ? 'bg-elevated text-highlighted' : 'hover:bg-elevated/60'"
              :aria-selected="isSelected(file.path, false)"
              :title="file.path"
              @click="onClick($event, file.path, false)"
              @dblclick="stage(covered(file.path, false))"
            >
              <span class="w-3 shrink-0 text-center font-mono text-label font-semibold" :class="file.conflicted ? 'text-error' : STATUS[file.status].class">{{ file.conflicted ? '!' : file.untracked ? '?' : STATUS[file.status].letter }}</span>
              <PathLabel :path="file.path" class="flex-1" />
              <UButton icon="i-lucide-undo-2" size="xs" color="neutral" variant="ghost" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :aria-label="t('LocalChanges.discard')" :title="t('LocalChanges.discard')" :disabled="busy" @click.stop="confirming = { paths: covered(file.path, false) }" />
              <UButton icon="i-lucide-plus" size="xs" color="neutral" variant="ghost" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :aria-label="t('LocalChanges.stage')" :title="t('LocalChanges.stage')" :disabled="busy" @click.stop="stage(covered(file.path, false))" />
            </li>
          </UContextMenu>
          <li v-if="!unstaged.length" class="px-3 py-1 text-label text-muted">{{ t('LocalChanges.nothingUnstaged') }}</li>
        </ul>
      </section>

      <section class="flex min-h-0 flex-col border-t border-default" :class="clean ? 'flex-none' : 'flex-1'">
        <header class="flex items-center gap-1 border-b border-default px-3 py-1.5">
          <h2 class="flex-1 text-label font-semibold tracking-wider text-muted uppercase">{{ t('LocalChanges.staged') }} <span class="font-normal tabular-nums">{{ staged.length }}</span></h2>
          <UButton v-if="selection.staged && selection.paths.size > 1" :label="t('LocalChanges.unstageSelected', { count: selection.paths.size })" size="xs" color="neutral" variant="soft" :disabled="busy" @click="unstage(covered(picked!.path, true))" />
          <UButton v-else-if="staged.length" :label="t('LocalChanges.unstageAll')" size="xs" color="neutral" variant="ghost" :disabled="busy" @click="unstage(staged.map(file => file.path))" />
        </header>
        <ul data-select-all="staged" class="min-h-0 flex-1 overflow-y-auto" :class="clean ? 'py-0.5' : 'py-1'">
          <UContextMenu v-for="file in staged" :key="file.path" :items="fileMenu(file, true)">
            <li
              class="group flex cursor-default items-center gap-2 px-3 py-1 text-ui"
              :class="isSelected(file.path, true) ? 'bg-elevated text-highlighted' : 'hover:bg-elevated/60'"
              :aria-selected="isSelected(file.path, true)"
              :title="file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path"
              @click="onClick($event, file.path, true)"
              @dblclick="unstage(covered(file.path, true))"
            >
              <span class="w-3 shrink-0 text-center font-mono text-label font-semibold" :class="STATUS[file.status].class">{{ STATUS[file.status].letter }}</span>
              <PathLabel :path="file.path" class="flex-1" />
              <UButton icon="i-lucide-minus" size="xs" color="neutral" variant="ghost" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :aria-label="t('LocalChanges.unstage')" :title="t('LocalChanges.unstage')" :disabled="busy" @click.stop="unstage(covered(file.path, true))" />
            </li>
          </UContextMenu>
          <li v-if="!staged.length" class="px-3 py-1 text-label text-muted">{{ t('LocalChanges.nothingStaged') }}</li>
        </ul>
      </section>

      <details v-if="discarded.length" class="border-t border-default px-3 py-2 text-ui">
        <summary class="cursor-pointer text-muted">{{ t('LocalChanges.recentlyDiscarded') }} <span class="tabular-nums">{{ discarded.length }}</span></summary>
        <ul data-select-all="discarded" class="mt-1 grid max-h-48 gap-1 overflow-y-auto">
          <li
            v-for="entry in discarded"
            :key="entry.id"
            class="flex cursor-default items-center gap-2 rounded pl-1"
            :class="discardedPick.several.value && discardedPick.isPicked(entry) ? 'bg-elevated text-highlighted' : ''"
            :aria-selected="discardedPick.several.value ? discardedPick.isPicked(entry) : undefined"
            @click="discardedPick.click(entry, $event)"
          >
            <span class="min-w-0 flex-1 truncate" :title="`${entry.paths.join('\n')}\n${when(entry.at)}`">
              {{ entry.paths.length === 1 ? entry.paths[0] : t('LocalChanges.files', { count: entry.paths.length }) }}
              <span class="text-label text-muted">{{ when(entry.at) }}</span>
            </span>
            <UButton :label="discardedPick.covering(entry).length > 1 ? t('LocalChanges.restoreSeveral', { count: discardedPick.covering(entry).length }) : t('common.restore')" size="xs" color="neutral" variant="outline" :disabled="busy" @click.stop="restoreAll(discardedPick.covering(entry).map(item => item.id))" />
          </li>
        </ul>
      </details>
    </aside>

    <div class="flex min-h-0 flex-col">
      <UAlert v-if="problem" class="m-2" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" :close="true" @update:open="problem = ''" />
      <LocalDiff
        v-if="current"
        ref="diffView"
        :key="`${current.path}:${picked?.staged}`"
        :path="current.path"
        :old-path="current.oldPath"
        :staged="!!picked?.staged"
        :untracked="current.untracked"
        :conflicted="current.conflicted"
        class="min-h-0 flex-1"
        @discard="askDiscard"
      />
      <LocalClean v-else-if="clean" @amend="amending = true" />
      <p v-else class="grid flex-1 place-items-center text-sm text-muted">{{ status ? t('LocalChanges.noChanges') : '' }}</p>
      <CommitBox v-if="!clean || amending" :key="amending ? 'amend' : 'commit'" :staged="staged.length" :amend="amending" class="border-t border-default" @done="amending = false" />
    </div>

    <UModal v-model:open="stashing" :title="t('LocalChanges.stashTitle')" :description="t('LocalChanges.stashHelp')">
      <template #body>
        <form class="grid gap-3" @submit.prevent="confirmStash">
          <ul class="grid max-h-72 gap-1 overflow-y-auto">
            <li v-for="file in changedFiles" :key="file.path">
              <UCheckbox :model-value="stashTicked.has(file.path)" :ui="{ label: 'font-mono text-xs' }" @update:model-value="tick(file.path, !!$event)">
                <template #label><span class="text-muted">{{ folder(file.path) }}</span>{{ name(file.path) }}<span v-if="file.untracked" class="text-muted"> · {{ t('LocalChanges.untracked') }}</span></template>
              </UCheckbox>
            </li>
          </ul>
          <UInput v-model="stashMessage" :placeholder="t('LocalChanges.stashMessage')" class="w-full" />
          <div class="flex justify-end gap-2">
            <UButton :label="t('common.cancel')" color="neutral" variant="ghost" @click="stashing = false" />
            <UButton type="submit" icon="i-lucide-archive" :label="t('LocalChanges.stashTicked', { count: stashTicked.size })" :disabled="!stashTicked.size" />
          </div>
        </form>
      </template>
    </UModal>

    <UModal v-model:open="open" :title="t('LocalChanges.discardTitle')" :description="t('LocalChanges.discardDescription', { what: confirmText })">
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton :label="t('LocalChanges.keep')" color="neutral" variant="ghost" @click="confirming = null" />
          <UButton :label="t('LocalChanges.discard')" color="error" variant="soft" icon="i-lucide-undo-2" @click="confirmDiscard" />
        </div>
      </template>
    </UModal>
  </div>
</template>
