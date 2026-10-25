<script setup lang="ts">
import type { FolderCheck, RecentRepo } from '~/bindings'

/*
 * Opening a repository in a tab: a folder picker, a path (checked while it is typed), and the ones opened before, each
 * with its branch, its uncommitted files and its last commit, to remove from the list or show in the file manager.
 * Typing in the path box also filters them; ↑ and ↓ go through them, Enter opens one, Esc goes back. Haunt keeps the
 * tabs between launches.
 */
const { scan } = useCode()
const { info: known, picking, chooseRoot } = useRepositories()
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

async function browse() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const chosen = await open({ directory: true, title: 'Open a repository', defaultPath: info.value.root ?? undefined })
  if (typeof chosen === 'string') {
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
const nameOf = (recent: string) => details.value.get(recent)?.name ?? recent.split(/[\\/]/).filter(Boolean).pop() ?? recent
/** Typing a path filters the recent ones by name or path. */
const shown = computed(() => {
  const term = path.value.trim().toLowerCase()
  return info.value.recent.filter(recent => !term || recent.toLowerCase().includes(term) || nameOf(recent).toLowerCase().includes(term))
})
async function forget(recent: string) {
  try {
    known.value = await haunt.forgetRecent(recent)
  } catch (error) {
    toast.add({ title: 'Not removed', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function reveal(recent: string) {
  try {
    await haunt.revealTab(recent)
  } catch (error) {
    toast.add({ title: 'Could not open the folder', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
const ago = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleDateString()
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return `${Math.max(1, minutes)} min ago`
  if (minutes < 60 * 24) return `${Math.floor(minutes / 60)} h ago`
  return `${Math.floor(minutes / 60 / 24)} d ago`
}

/* The typed path, checked a moment after typing stops. */
const check = shallowRef<FolderCheck | null>(null)
let checking: ReturnType<typeof setTimeout> | undefined
watch(path, (now) => {
  clearTimeout(checking)
  check.value = null
  problem.value = ''
  // A few letters filter the recent list; a path (with a slash or a drive) is checked.
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
  if (!found.exists) return { color: 'text-error', icon: 'i-lucide-circle-x', text: 'No folder there.' }
  if (!found.openable) return { color: 'text-warning', icon: 'i-lucide-triangle-alert', text: 'A folder, but no project or git repository in it.' }
  return { color: 'text-success', icon: 'i-lucide-circle-check', text: found.repository ? 'A git repository: Enter opens it.' : 'A project: Enter opens it.' }
})

/* The keyboard: ↑ ↓ through the recent ones, Enter opens the one picked (else the path), Esc goes back. */
const active = ref(-1)
watch(shown, () => { active.value = -1 })
function onKey(event: KeyboardEvent) {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    if (!shown.value.length) return
    event.preventDefault()
    const step = event.key === 'ArrowDown' ? 1 : -1
    active.value = (active.value + step + shown.value.length) % shown.value.length
  } else if (event.key === 'Escape' && picking.value && scan.value) {
    picking.value = false
  }
}
function submit() {
  const picked = shown.value[active.value]
  if (picked) void use(picked)
  else if (path.value.trim()) void use(path.value.trim())
}
onMounted(() => nextTick(() => pathBox.value?.inputRef?.focus()))
</script>

<template>
  <div class="mx-auto grid max-w-2xl gap-6 px-4 py-14" @keydown="onKey">
    <div class="grid gap-2">
      <div class="flex items-center justify-between gap-2">
        <h1 class="text-xl font-semibold text-highlighted">Open a repository</h1>
        <!-- With no repository open there is no toolbar: Settings (and Ctrl+,) are here then, for a new profile too. -->
        <SettingsMenu />
      </div>
      <p class="text-sm text-muted">
        Pick a project's root: the folder with its <code>package.json</code>, <code>tsconfig.json</code>,
        <code>composer.json</code> or <code>Cargo.toml</code>. Workspaces are drawn by package. Install dependencies
        first so imports resolve.
      </p>
    </div>

    <UButton v-if="inApp" icon="i-lucide-folder-open" label="Choose a folder" size="lg" class="justify-self-start" :loading="busy" @click="browse" />

    <div class="grid gap-1.5">
      <form class="flex gap-2" @submit.prevent="submit">
        <UInput
          ref="pathBox"
          v-model="path"
          :placeholder="info.recent.length ? 'A path, or letters to find a recent one' : 'C:\\Users\\you\\Projects\\my-app'"
          class="flex-1"
          :ui="{ base: 'font-mono' }"
          aria-label="Path or filter"
        />
        <UButton type="submit" label="Open" color="neutral" variant="outline" :disabled="!path.trim() && active < 0" :loading="busy && !inApp" />
      </form>
      <p v-if="verdict" class="flex items-center gap-1.5 text-xs" :class="verdict.color"><UIcon :name="verdict.icon" class="size-3.5" />{{ verdict.text }}</p>
    </div>

    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />

    <section v-if="info.recent.length" class="grid gap-2">
      <h2 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
        Recent
        <span v-if="path.trim() && shown.length < info.recent.length" class="font-normal tracking-normal normal-case">{{ shown.length }} of {{ info.recent.length }}</span>
        <span class="ml-auto font-normal tracking-normal normal-case">↑ ↓ to pick, Enter to open</span>
      </h2>
      <p v-if="!shown.length" class="text-sm text-muted">No recent repository matches; Enter opens the path.</p>
      <ul class="grid gap-1">
        <li v-for="(recent, at) in shown" :key="recent" class="group relative">
          <button
            type="button"
            class="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 rounded-md border px-3 py-2 text-left"
            :class="[at === active ? 'border-primary bg-elevated' : 'border-default hover:bg-elevated/60', details.get(recent)?.openable === false ? 'opacity-60' : '']"
            :title="recent"
            @click="use(recent)"
            @mouseenter="active = at"
          >
            <UIcon :name="details.get(recent)?.branch ? 'i-lucide-folder-git-2' : 'i-lucide-folder'" class="size-5 text-muted" />
            <span class="grid min-w-0">
              <span class="flex min-w-0 items-center gap-2">
                <span class="truncate font-medium text-highlighted">{{ nameOf(recent) }}</span>
                <UBadge v-if="info.tabs.includes(recent)" label="in a tab" size="sm" variant="subtle" color="neutral" />
                <UBadge v-if="details.get(recent)?.openable === false" label="not found" size="sm" variant="subtle" color="warning" />
              </span>
              <span class="truncate font-mono text-label text-muted">{{ recent }}</span>
            </span>
            <span class="grid justify-items-end gap-0.5 pr-14 text-label text-muted">
              <span v-if="details.get(recent)?.branch" class="flex items-center gap-1 font-mono"><UIcon name="i-lucide-git-branch" class="size-3" />{{ details.get(recent)!.branch }}</span>
              <span class="flex items-center gap-2">
                <span v-if="details.get(recent)?.changes" class="flex items-center gap-1 text-warning" :title="`${details.get(recent)!.changes} files not committed yet`"><UIcon name="i-lucide-pencil-line" class="size-3" />{{ details.get(recent)!.changes }}</span>
                <span v-if="details.get(recent)?.lastCommit" :title="details.get(recent)!.lastCommit!.subject">{{ ago(details.get(recent)!.lastCommit!.time) }}</span>
              </span>
            </span>
          </button>
          <span class="absolute top-1/2 right-2 flex -translate-y-1/2 gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
            <UButton v-if="details.get(recent)?.openable !== false" icon="i-lucide-folder-open" size="xs" color="neutral" variant="ghost" :aria-label="`Show ${nameOf(recent)} in the file manager`" title="Show in the file manager" @click="reveal(recent)" />
            <UButton icon="i-lucide-x" size="xs" color="neutral" variant="ghost" :aria-label="`Remove ${nameOf(recent)} from the recent list`" title="Remove from the list (the folder stays)" @click="forget(recent)" />
          </span>
        </li>
      </ul>
    </section>

    <p class="text-xs text-muted">You can also drop a folder on this window, or run <code>haunt &lt;folder&gt;</code> in a terminal.</p>

    <UButton v-if="picking && scan" label="Back" color="neutral" variant="ghost" class="justify-self-start" @click="picking = false">
      <template #trailing><UKbd value="Esc" size="sm" /></template>
    </UButton>
  </div>
</template>
