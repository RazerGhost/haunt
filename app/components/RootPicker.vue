<script setup lang="ts">
import type { FolderCheck, RecentRepo, RepoFolder } from '~/bindings'
import { repoTree, type RepoTree } from '~~/shared/repositories'

/*
 * The repository manager's left side: a path box (checked while it is typed, and a filter for the lists), a folder
 * picker, Clone and New (RepoCreate), the recent repositories, each with its branch, its uncommitted files and its
 * last commit, and the repositories in your repository folders, as a tree of their folders. A click picks one (shown
 * on the right), a double click or Enter opens it in a tab; ↑ and ↓ go through them, Esc goes back. Haunt keeps the
 * tabs between launches.
 */
const picked = defineModel<string | null>('picked', { required: true })
const { scan } = useCode()
const { info: known, picking, chooseRoot, onNews } = useRepositories()
const { times } = usePreferences()
const toast = useToast()
const info = computed(() => ({ root: known.value?.root ?? null, tabs: known.value?.tabs ?? [], recent: known.value?.recent ?? [] }))

const inApp = import.meta.client && '__TAURI__' in window
const path = ref('')
const problem = ref('')
const busy = ref(false)
const pathBox = ref<{ inputRef?: HTMLInputElement } | null>(null)

async function use(chosen: string) {
  busy.value = true
  problem.value = (await chooseRoot(chosen)) ?? ''
  busy.value = false
}

async function chooseFolder(title: string): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const chosen = await open({ directory: true, title, defaultPath: info.value.root ?? undefined })
  return typeof chosen === 'string' ? chosen : null
}

async function browse() {
  const chosen = await chooseFolder(t('RootPicker.title'))
  if (chosen) {
    path.value = chosen
    await use(chosen)
  }
}

/* The recent ones, with what each is now. */
const details = shallowRef<Map<string, RecentRepo>>(new Map())
watch(() => info.value.recent.join('\n'), async () => {
  try {
    details.value = new Map((await haunt.recentDetails()).map(item => [item.path, item]))
  } catch { /* shown without them */ }
}, { immediate: true })
const folderName = (repo: string) => repo.split(/[\\/]/).filter(Boolean).pop() ?? repo
const nameOf = (recent: string) => details.value.get(recent)?.name ?? folderName(recent)
/** Typing letters filters both lists by name or path; a path is checked instead. */
const term = computed(() => path.value.trim().toLowerCase())
const matches = (repo: string, name: string) => !term.value || repo.toLowerCase().includes(term.value) || name.toLowerCase().includes(term.value)
const shown = computed(() => info.value.recent.filter(recent => matches(recent, nameOf(recent))))
async function forget(recent: string) {
  try {
    known.value = await haunt.forgetRecent(recent)
  } catch (error) {
    toast.add({ title: t('RootPicker.notRemoved'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function reveal(repo: string) {
  try {
    await haunt.revealTab(repo)
  } catch (error) {
    toast.add({ title: t('RootPicker.folderNotOpened'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
const ago = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleDateString(locale())
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return t('RootPicker.minutesAgo', { count: Math.max(1, minutes) })
  if (minutes < 60 * 24) return t('RootPicker.hoursAgo', { count: Math.floor(minutes / 60) })
  return t('RootPicker.daysAgo', { count: Math.floor(minutes / 60 / 24) })
}

/* The repository folders, each as a tree; a folder or a tree folder can be folded away. */
const folders = shallowRef<RepoFolder[]>([])
const loadingFolders = ref(true)
async function loadFolders(change?: Promise<RepoFolder[]>) {
  try {
    folders.value = await (change ?? haunt.repositoryFolders())
  } catch (error) {
    toast.add({ title: t('RootPicker.foldersNotRead'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  } finally {
    loadingFolders.value = false
  }
}
onMounted(() => void loadFolders())
// A background search found a different list of repositories in a folder.
onBeforeUnmount(onNews((news) => { if (news.type === 'folders') void loadFolders() }))
async function addFolder() {
  const chosen = inApp ? await chooseFolder(t('RootPicker.addFolder')) : path.value.trim()
  if (chosen) await loadFolders(haunt.addRepositoryFolder(chosen))
}
const removeFolder = (folder: string) => loadFolders(haunt.removeRepositoryFolder(folder))
const shut = usePersisted<string[]>('manager-shut', [], value => Array.isArray(value) && value.every(item => typeof item === 'string'))
function toggle(key: string) {
  shut.value = shut.value.includes(key) ? shut.value.filter(item => item !== key) : [...shut.value, key]
}
interface Row { key: string, depth: number, name: string, repo: string | null, open: boolean }
/** Each folder's tree as rows, folded folders left out; while filtering, every match shows. */
const trees = computed(() => folders.value.map((folder) => {
  const tree = repoTree(folder.path, folder.repos.filter(repo => matches(repo, folderName(repo))))
  const rows: Row[] = []
  const walk = (node: RepoTree, depth: number, key: string) => {
    for (const child of node.children) {
      const childKey = `${key}/${child.name}`
      const open = !!term.value || !shut.value.includes(childKey)
      rows.push({ key: childKey, depth, name: child.name, repo: child.repo, open })
      if (!child.repo && open) walk(child, depth + 1, childKey)
    }
  }
  walk(tree, 0, folder.path)
  return { folder, name: tree.name, open: !!term.value || !shut.value.includes(folder.path), rows, count: tree.children.length }
}))

/* The typed path, checked a moment after typing stops. */
const check = shallowRef<FolderCheck | null>(null)
let checking: ReturnType<typeof setTimeout> | undefined
watch(path, (now) => {
  clearTimeout(checking)
  check.value = null
  problem.value = ''
  // A few letters filter the lists; a path (with a slash or a drive) is checked.
  if (!/[\\/]|^[a-z]:/i.test(now.trim())) return
  checking = setTimeout(async () => {
    try {
      check.value = await haunt.checkFolder(now.trim())
    } catch { /* unchecked */ }
  }, 250)
})
const verdict = computed(() => {
  const found = check.value
  if (!found) return null
  if (!found.exists) return { color: 'text-error', icon: 'i-lucide-circle-x', text: t('RootPicker.noFolder') }
  if (!found.openable) return { color: 'text-warning', icon: 'i-lucide-triangle-alert', text: t('RootPicker.notOpenable') }
  return { color: 'text-success', icon: 'i-lucide-circle-check', text: found.repository ? t('RootPicker.isRepository') : t('RootPicker.isProject') }
})

/* The keyboard: ↑ ↓ through every repository listed, Enter opens the one picked (else the path), Esc goes back. */
const listed = computed(() => [...shown.value, ...trees.value.flatMap(tree => tree.open ? tree.rows.flatMap(row => row.repo ? [row.repo] : []) : [])])
function onKey(event: KeyboardEvent) {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    if (!listed.value.length) return
    event.preventDefault()
    const at = picked.value ? listed.value.indexOf(picked.value) : -1
    const step = event.key === 'ArrowDown' ? 1 : -1
    picked.value = listed.value[(at + step + listed.value.length) % listed.value.length] ?? null
    void nextTick(() => document.querySelector('[data-picked="true"]')?.scrollIntoView({ block: 'nearest' }))
  } else if (event.key === 'Escape' && picking.value && scan.value) {
    picking.value = false
  }
}
function submit() {
  if (path.value.trim() && !listed.value.includes(picked.value ?? '')) void use(path.value.trim())
  else if (picked.value) void use(picked.value)
}

/* Clone and New, in their dialog. */
const creating = ref<'clone' | 'new' | null>(null)
const createOpen = computed({ get: () => creating.value !== null, set: (open) => { if (!open) creating.value = null } })
onMounted(() => nextTick(() => pathBox.value?.inputRef?.focus()))
</script>

<template>
  <div class="flex min-h-0 flex-col gap-3 border-r border-default p-3" @keydown="onKey">
    <div class="flex items-center justify-between gap-2">
      <h1 class="text-base font-semibold text-highlighted">{{ t('RootPicker.title') }}</h1>
      <!-- With no repository open there is no toolbar: Settings (and Ctrl+,) are here then, for a new profile too. -->
      <SettingsMenu />
    </div>

    <div class="grid gap-1.5">
      <form class="flex gap-2" @submit.prevent="submit">
        <UInput
          ref="pathBox"
          v-model="path"
          :placeholder="t('RootPicker.pathOrFilter')"
          icon="i-lucide-search"
          size="sm"
          class="flex-1"
          :ui="{ base: 'font-mono' }"
          :aria-label="t('RootPicker.pathLabel')"
        />
      </form>
      <p v-if="verdict" class="flex items-center gap-1.5 text-xs" :class="verdict.color"><UIcon :name="verdict.icon" class="size-3.5" />{{ verdict.text }}</p>
    </div>

    <div class="flex flex-wrap gap-1.5">
      <UButton v-if="inApp" icon="i-lucide-folder-open" :label="t('RootPicker.chooseFolder')" size="sm" color="neutral" variant="outline" :loading="busy" @click="browse" />
      <UButton icon="i-lucide-download" :label="t('RootPicker.clone')" size="sm" color="neutral" variant="outline" @click="creating = 'clone'" />
      <UButton icon="i-lucide-folder-plus" :label="t('RootPicker.new')" size="sm" color="neutral" variant="outline" @click="creating = 'new'" />
    </div>

    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />

    <div class="-mx-1 grid min-h-0 flex-1 content-start gap-4 overflow-auto px-1">
      <section v-if="info.recent.length" class="grid gap-1">
        <h2 class="flex items-center gap-2 text-label font-semibold tracking-wider text-muted uppercase">
          {{ t('RootPicker.recent') }}
          <span v-if="term && shown.length < info.recent.length" class="font-normal tracking-normal normal-case">{{ t('RootPicker.shownOf', { shown: shown.length, total: info.recent.length }) }}</span>
        </h2>
        <p v-if="!shown.length" class="text-xs text-muted">{{ t('RootPicker.noMatch') }}</p>
        <ul class="grid gap-0.5">
          <!-- The row's buttons take their own room at its end (shown on hover, focus or when picked), so the name
               shortens instead of being covered; its status gives way to them. -->
          <li
            v-for="recent in shown"
            :key="recent"
            class="group flex min-w-0 items-center rounded-md"
            :class="[picked === recent ? 'bg-elevated' : 'hover:bg-elevated/60', details.get(recent)?.openable === false ? 'opacity-60' : '']"
          >
            <button
              type="button"
              class="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 px-2 py-1.5 text-left"
              :data-picked="picked === recent"
              :title="recent"
              @click="picked = recent"
              @dblclick="use(recent)"
            >
              <UIcon :name="details.get(recent)?.branch ? 'i-lucide-folder-git-2' : 'i-lucide-folder'" class="size-4 text-muted" />
              <span class="grid min-w-0">
                <span class="flex min-w-0 items-center gap-1.5">
                  <span class="truncate text-sm font-medium text-highlighted">{{ nameOf(recent) }}</span>
                  <UBadge v-if="info.tabs.includes(recent)" :label="t('RootPicker.inTab')" size="sm" variant="subtle" color="neutral" class="shrink-0" />
                  <UBadge v-if="details.get(recent)?.openable === false" :label="t('RootPicker.notFound')" size="sm" variant="subtle" color="warning" class="shrink-0" />
                </span>
                <span v-if="details.get(recent)?.branch" class="flex min-w-0 items-center gap-1 font-mono text-label text-muted"><UIcon name="i-lucide-git-branch" class="size-3 shrink-0" /><span class="truncate">{{ details.get(recent)!.branch }}</span></span>
              </span>
              <span class="grid justify-items-end gap-0.5 text-label text-muted group-focus-within:hidden group-hover:hidden" :class="{ hidden: picked === recent }">
                <span v-if="details.get(recent)?.changes" class="flex items-center gap-1 text-warning" :title="t('RootPicker.uncommitted', { count: details.get(recent)!.changes! })"><UIcon name="i-lucide-pencil-line" class="size-3" />{{ details.get(recent)!.changes }}</span>
                <span v-if="details.get(recent)?.lastCommit" :title="details.get(recent)!.lastCommit!.subject">{{ ago(details.get(recent)!.lastCommit!.time) }}</span>
              </span>
            </button>
            <span class="hidden shrink-0 gap-0.5 pr-1 group-focus-within:flex group-hover:flex" :class="{ flex: picked === recent }">
              <UTooltip v-if="details.get(recent)?.openable !== false" :text="t('RootPicker.openNamed', { name: nameOf(recent) })">
                <UButton icon="i-lucide-arrow-up-right" size="xs" color="primary" variant="soft" :aria-label="t('RootPicker.openNamed', { name: nameOf(recent) })" @click="use(recent)" />
              </UTooltip>
              <UTooltip v-if="details.get(recent)?.openable !== false" :text="t('RootPicker.reveal')">
                <UButton icon="i-lucide-folder-open" size="xs" color="neutral" variant="ghost" :aria-label="t('RootPicker.revealNamed', { name: nameOf(recent) })" @click="reveal(recent)" />
              </UTooltip>
              <UTooltip :text="t('RootPicker.forget')">
                <UButton icon="i-lucide-x" size="xs" color="neutral" variant="ghost" :aria-label="t('RootPicker.forgetNamed', { name: nameOf(recent) })" @click="forget(recent)" />
              </UTooltip>
            </span>
          </li>
        </ul>
      </section>

      <section class="grid gap-1">
        <h2 class="flex items-center gap-2 text-label font-semibold tracking-wider text-muted uppercase">
          {{ t('RootPicker.folders') }}
          <UButton icon="i-lucide-plus" size="xs" color="neutral" variant="ghost" class="ml-auto" :disabled="!inApp && !path.trim()" :aria-label="t('RootPicker.addFolder')" :title="inApp ? t('RootPicker.addFolder') : t('RootPicker.addTypedFolder')" @click="addFolder" />
        </h2>
        <p v-if="!loadingFolders && !folders.length" class="text-xs text-muted">{{ t('RootPicker.noFolders') }}</p>
        <div v-for="tree in trees" :key="tree.folder.path" class="grid gap-0.5">
          <div class="group flex min-w-0 items-center gap-1">
            <button type="button" class="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 py-1 text-left text-sm hover:bg-elevated/60" :title="tree.folder.path" :aria-expanded="tree.open" @click="toggle(tree.folder.path)">
              <UIcon name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-muted transition" :class="{ 'rotate-90': tree.open }" />
              <UIcon name="i-lucide-folder-tree" class="size-4 shrink-0 text-muted" />
              <span class="truncate font-medium text-highlighted">{{ tree.name }}</span>
              <UBadge v-if="tree.folder.missing" :label="t('RootPicker.notFound')" size="sm" variant="subtle" color="warning" />
              <span class="ml-auto text-label text-muted tabular-nums">{{ tree.folder.repos.length }}{{ tree.folder.cut ? '+' : '' }}</span>
            </button>
            <UButton icon="i-lucide-x" size="xs" color="neutral" variant="ghost" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :aria-label="t('RootPicker.removeFolderNamed', { name: tree.name })" :title="t('RootPicker.removeFolder')" @click="removeFolder(tree.folder.path)" />
          </div>
          <template v-if="tree.open">
            <p v-if="!tree.rows.length && !tree.folder.missing" class="pl-7 text-xs text-muted">{{ term ? t('RootPicker.noFolderMatch') : t('RootPicker.noRepos') }}</p>
            <div v-for="row in tree.rows" :key="row.key" class="group flex min-w-0 items-center rounded-md" :class="row.repo && picked === row.repo ? 'bg-elevated' : 'hover:bg-elevated/60'">
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-1.5 py-1 pr-2 text-left text-sm"
              :style="{ paddingLeft: `${1.25 + row.depth * 1}rem` }"
              :data-picked="!!row.repo && picked === row.repo"
              :title="row.repo ?? undefined"
              :aria-expanded="row.repo ? undefined : row.open"
              @click="row.repo ? (picked = row.repo) : toggle(row.key)"
              @dblclick="row.repo && use(row.repo)"
            >
              <UIcon v-if="!row.repo" name="i-lucide-chevron-right" class="size-3.5 shrink-0 text-muted transition" :class="{ 'rotate-90': row.open }" />
              <UIcon :name="row.repo ? 'i-lucide-folder-git-2' : 'i-lucide-folder'" class="size-4 shrink-0 text-muted" />
              <span class="truncate" :class="row.repo ? 'text-highlighted' : 'text-muted'">{{ row.name }}</span>
              <UBadge v-if="row.repo && info.tabs.includes(row.repo)" :label="t('RootPicker.inTab')" size="sm" variant="subtle" color="neutral" class="ml-auto shrink-0 group-focus-within:hidden group-hover:hidden" :class="{ hidden: picked === row.repo }" />
            </button>
            <UTooltip v-if="row.repo" :text="t('RootPicker.openNamed', { name: row.name })">
              <UButton
                icon="i-lucide-arrow-up-right"
                size="xs"
                color="primary"
                variant="soft"
                class="mr-1 hidden shrink-0 group-focus-within:inline-flex group-hover:inline-flex"
                :class="{ 'inline-flex': picked === row.repo }"
                :aria-label="t('RootPicker.openNamed', { name: row.name })"
                @click="use(row.repo)"
              />
            </UTooltip>
            </div>
          </template>
        </div>
      </section>
    </div>

    <div class="grid gap-2">
      <p class="text-label text-muted">{{ t('RootPicker.keys') }}</p>
      <p class="text-label text-muted">{{ t('RootPicker.dropBefore') }} <code>haunt &lt;folder&gt;</code> {{ t('RootPicker.dropAfter') }}</p>
      <UButton v-if="picking && scan" :label="t('RootPicker.back')" color="neutral" variant="ghost" size="sm" class="justify-self-start" @click="picking = false">
        <template #trailing><UKbd value="Esc" size="sm" /></template>
      </UButton>
    </div>

    <RepoCreate v-model:open="createOpen" :mode="creating ?? 'clone'" :folders="folders.filter(folder => !folder.missing).map(folder => folder.path)" @created="use" />
  </div>
</template>
