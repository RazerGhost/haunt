<script setup lang="ts">
/*
 * The window's top row: a tab per open repository, a + to open another, and the window's own buttons (on macOS the
 * system's, at the left). Drag the empty space to move the window, double-click it to maximise. Drag a tab to move it;
 * right-click one to pin it (first in the row, its name hidden, no close button) or give it a colour, to close it, the
 * others or those to its right (never a pinned one), to move it to another profile, or to show its folder in the file
 * manager or open a terminal there. With more than one profile, the profile in use is first in the row: a menu to switch, add,
 * rename or delete them (settings.rs keeps each profile's tabs).
 */
const { info, picking, closed, chooseRoot, closeTab, reopenClosed } = useRepositories()
const dialog = useDialog()
const failure = useFailure()

/* Profiles. */
const others = computed(() => (info.value?.profiles ?? []).filter(name => name !== info.value?.profile))
async function profiles(change: () => Promise<typeof info.value>, what: string): Promise<void> {
  try {
    info.value = await change()
    picking.value = false
  } catch (error) {
    failure.failed(what, error, t('TitleBar.profilesUnchanged'))
  }
}
async function newProfile(): Promise<void> {
  const answer = await dialog.prompt({
    title: t('TitleBar.newProfile'),
    description: t('TitleBar.newProfileHelp'),
    input: { label: t('TitleBar.name'), value: '', placeholder: t('TitleBar.namePlaceholder') },
    confirm: t('TitleBar.createAndSwitch')
  })
  if (answer?.text.trim()) await profiles(() => haunt.newProfile(answer.text.trim()), t('TitleBar.profileNotMade'))
}
async function renameProfile(): Promise<void> {
  const current = info.value?.profile ?? ''
  const answer = await dialog.prompt({ title: t('TitleBar.renameTitle', { name: current }), input: { label: t('TitleBar.newName'), value: current }, confirm: t('TitleBar.rename') })
  if (answer?.text.trim()) await profiles(() => haunt.renameProfile(current, answer.text.trim()), t('TitleBar.profileNotRenamed'))
}
async function deleteProfile(name: string): Promise<void> {
  const ok = await dialog.confirm({ title: t('TitleBar.deleteTitle', { name }), description: t('TitleBar.deleteHelp'), confirm: t('TitleBar.delete'), danger: true })
  if (ok) await profiles(() => haunt.deleteProfile(name), t('TitleBar.profileNotDeleted'))
}
const profileMenu = computed(() => [
  others.value.map(name => ({ label: name, icon: 'i-lucide-user-round', onSelect: () => profiles(() => haunt.switchProfile(name), t('TitleBar.profileNotSwitched')) })),
  [
    { label: t('TitleBar.newProfileMenu'), icon: 'i-lucide-user-round-plus', onSelect: newProfile },
    { label: t('TitleBar.renameMenu', { name: info.value?.profile ?? '' }), icon: 'i-lucide-pencil', onSelect: renameProfile },
    ...others.value.length ? [{ label: t('TitleBar.deleteProfile'), icon: 'i-lucide-trash-2', children: others.value.map(name => ({ label: name, color: 'error' as const, onSelect: () => deleteProfile(name) })) }] : []
  ]
])
defineShortcuts({ meta_shift_t: () => { void reopenClosed() } })
/** The + menu: a repository to open, the recent ones not open, and the tab closed last. */
const openMenu = computed(() => {
  const name = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path
  const recent = (info.value?.recent ?? []).filter(path => !(info.value?.tabs ?? []).includes(path)).slice(0, 8)
  const last = closed.value.at(-1)
  return [
    [{ label: t('TitleBar.openRepository'), icon: 'i-lucide-folder-open', onSelect: () => { picking.value = true } }],
    ...last ? [[{ label: t('TitleBar.reopen', { name: name(last) }), icon: 'i-lucide-rotate-ccw', kbds: ['meta', 'shift', 't'], onSelect: () => { void reopenClosed() } }]] : [],
    ...recent.length ? [recent.map(path => ({ label: name(path), suffix: path, icon: 'i-lucide-folder-git-2', onSelect: () => { void chooseRoot(path) } }))] : [],
    // The first extra profile starts here; after that the profile menu has it too.
    ...others.value.length ? [] : [[{ label: t('TitleBar.newProfileMenu'), icon: 'i-lucide-user-round-plus', onSelect: newProfile }]]
  ]
})
const { inApp, mac } = useDesktop()

const tabs = computed(() => {
  const paths = info.value?.tabs ?? []
  const name = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path
  return paths.map((path) => {
    // Two tabs with the same folder name also show the folder they are in.
    const parts = path.split(/[\\/]/).filter(Boolean)
    const label = paths.filter(other => name(other) === name(path)).length > 1 ? parts.slice(-2).join('/') : name(path)
    const look = info.value?.tab_looks[path]
    return { path, label, active: path === info.value?.root && !picking.value, worktree: info.value?.worktrees.includes(path) ?? false, pinned: !!look?.pinned, color: look?.color ?? '' }
  })
})

/* Pinned and coloured tabs (settings.rs keeps them by folder). */
/** Tailwind's 500 shades, as Settings shows them (the window only has the variables of colours it uses). */
const SHADES: Record<string, string> = { red: '#fb2c36', orange: '#ff6900', amber: '#fe9a00', green: '#00c950', teal: '#00bba7', blue: '#2b7fff', violet: '#8e51ff', pink: '#f6339a' }
const COLORS = Object.keys(SHADES)
const pinned = (path: string) => !!info.value?.tab_looks[path]?.pinned
async function setLook(path: string, change: { pinned?: boolean, color?: string }): Promise<void> {
  const now = info.value?.tab_looks[path] ?? { pinned: false, color: '' }
  try {
    info.value = await haunt.setTabLook(path, { ...now, ...change })
  } catch (error) {
    failure.failed(t('TitleBar.tabUnchanged'), error)
  }
}

const toast = useToast()
function tabMenu(path: string) {
  const all = info.value?.tabs ?? []
  const at = all.indexOf(path)
  const closeAll = async (paths: string[]) => { for (const other of paths) await closeTab(other) }
  const rest = all.filter(other => other !== path && !pinned(other))
  const right = all.slice(at + 1).filter(other => !pinned(other))
  const color = info.value?.tab_looks[path]?.color ?? ''
  return [
    [
      pinned(path)
        ? { label: t('TitleBar.unpin'), icon: 'i-lucide-pin-off', onSelect: () => setLook(path, { pinned: false }) }
        : { label: t('TitleBar.pin'), icon: 'i-lucide-pin', onSelect: () => setLook(path, { pinned: true }) },
      {
        label: t('TitleBar.colour'),
        icon: 'i-lucide-palette',
        children: [
          { label: t('TitleBar.none'), type: 'checkbox' as const, checked: !color, onSelect: () => setLook(path, { color: '' }) },
          ...COLORS.map(name => ({ label: t(`TitleBar.colours.${name}`), type: 'checkbox' as const, checked: color === name, onSelect: () => setLook(path, { color: name }) }))
        ]
      }
    ],
    [
      ...pinned(path) ? [] : [{ label: t('common.close'), icon: 'i-lucide-x', onSelect: () => closeTab(path) }],
      { label: t('TitleBar.closeOthers'), icon: 'i-lucide-copy-x', disabled: !rest.length, onSelect: () => closeAll(rest) },
      { label: t('TitleBar.closeRight'), icon: 'i-lucide-arrow-right-to-line', disabled: !right.length, onSelect: () => closeAll(right) }
    ],
    [
      ...others.value.length
        ? [{ label: t('TitleBar.moveToProfile'), icon: 'i-lucide-user-round', children: others.value.map(name => ({ label: name, onSelect: () => profiles(() => haunt.moveTab(path, name), t('TitleBar.tabNotMoved')) })) }]
        : [],
      { label: t('TitleBar.reveal'), icon: 'i-lucide-folder-open', onSelect: () => reveal(path) },
      { label: t('TitleBar.terminal'), icon: 'i-lucide-square-terminal', onSelect: () => openTerminal(path) }
    ]
  ]
}
async function openTerminal(path: string): Promise<void> {
  try {
    await haunt.terminalTab(path)
  } catch (error) {
    toast.add({ title: t('TitleBar.terminalFailed'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function reveal(path: string): Promise<void> {
  try {
    await haunt.revealTab(path)
  } catch (error) {
    toast.add({ title: t('TitleBar.revealFailed'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}

// Dragging a tab onto another puts it there.
const dragging = ref<string | null>(null)
async function dropOn(path: string): Promise<void> {
  const moved = dragging.value
  dragging.value = null
  const all = [...info.value?.tabs ?? []]
  if (!moved || moved === path || !all.includes(moved)) return
  all.splice(all.indexOf(moved), 1)
  all.splice(all.indexOf(path) + (all.indexOf(path) >= (info.value?.tabs ?? []).indexOf(moved) ? 1 : 0), 0, moved)
  try {
    info.value = await haunt.reorderTabs(all)
  } catch { /* the tabs changed meanwhile: they show as they are */ }
}

function show(path: string): void {
  if (path === info.value?.root) picking.value = false
  else void chooseRoot(path)
}
</script>

<template>
  <div data-tauri-drag-region class="flex h-10 shrink-0 items-stretch border-b border-default bg-elevated/40 select-none" :class="inApp && mac ? 'pl-20' : 'pl-2'">
    <UDropdownMenu v-if="others.length" :items="profileMenu" :content="{ align: 'start' }">
      <button
        type="button"
        class="mt-1.5 mr-1 mb-1 flex max-w-36 items-center gap-1 self-center rounded-md px-2 py-1 text-xs text-muted hover:bg-elevated hover:text-highlighted"
        :title="t('TitleBar.profileHelp', { name: info?.profile ?? '' })"
      >
        <UIcon name="i-lucide-user-round" class="size-3.5 shrink-0" />
        <span class="truncate font-medium">{{ info?.profile }}</span>
        <UIcon name="i-lucide-chevron-down" class="size-3 shrink-0" />
      </button>
    </UDropdownMenu>
    <nav class="flex min-w-0 items-end gap-0.5 pt-1.5" :aria-label="t('TitleBar.openRepositories')">
      <UContextMenu v-for="tab in tabs" :key="tab.path" :items="tabMenu(tab.path)">
      <div
        class="group flex h-full items-center gap-1 rounded-t-md border border-b-0 text-sm"
        :class="[tab.pinned ? 'px-2.5' : 'max-w-56 min-w-28 pr-1 pl-3', tab.active ? 'border-default bg-default text-highlighted' : 'border-transparent text-muted hover:bg-elevated hover:text-default', dragging && dragging !== tab.path ? 'outline-dashed outline-1 outline-accented' : '']"
        :style="tab.color ? { boxShadow: `inset 0 2px 0 ${SHADES[tab.color]}` } : undefined"
        draggable="true"
        @dragstart="dragging = tab.path"
        @dragend="dragging = null"
        @dragover.prevent
        @drop.prevent="dropOn(tab.path)"
      >
        <button type="button" class="flex min-w-0 flex-1 items-center gap-1.5 text-left" :title="tab.pinned ? t('TitleBar.pinnedTitle', { name: tab.label, path: tab.path }) : tab.path" :aria-label="tab.pinned ? tab.label : undefined" :aria-current="tab.active ? 'page' : undefined" @click="show(tab.path)" @auxclick.middle="tab.pinned || closeTab(tab.path)">
          <UIcon
            :name="tab.worktree ? 'i-lucide-git-fork' : 'i-lucide-folder-git-2'"
            class="size-3.5 shrink-0"
            :class="tab.color ? '' : tab.active ? 'text-highlighted' : 'text-dimmed'"
            :style="tab.color ? { color: SHADES[tab.color] } : undefined"
            :title="tab.worktree ? t('TitleBar.worktree') : undefined"
          />
          <span v-if="!tab.pinned" class="truncate">{{ tab.label }}</span>
        </button>
        <button
          v-if="!tab.pinned"
          type="button"
          class="grid size-5 shrink-0 place-items-center rounded text-dimmed hover:bg-accented hover:text-highlighted focus-visible:opacity-100"
          :class="tab.active ? '' : 'opacity-0 group-hover:opacity-100'"
          :aria-label="t('TitleBar.closeTab', { name: tab.label })"
          @click="closeTab(tab.path)"
        >
          <UIcon name="i-lucide-x" class="size-3" />
        </button>
      </div>
      </UContextMenu>
      <UDropdownMenu :items="openMenu" :content="{ align: 'start' }">
        <button
          type="button"
          class="mb-1 ml-1 grid size-7 place-items-center rounded-md text-muted hover:bg-elevated hover:text-highlighted"
          :class="{ 'bg-elevated text-highlighted': picking }"
          :aria-label="t('TitleBar.openRepositoryLabel')"
          :title="t('TitleBar.openHelp')"
        >
          <UIcon name="i-lucide-plus" class="size-4" />
        </button>
      </UDropdownMenu>
    </nav>
    <div data-tauri-drag-region class="min-w-8 flex-1" />
    <WindowControls v-if="inApp && !mac" />
  </div>
</template>
