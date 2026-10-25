<script setup lang="ts">
/*
 * The command palette (Ctrl+K, ⌘K on macOS): one search over what Haunt can do and where it can go. Actions (undo,
 * fetch, pull, push, a terminal, a new branch, stash, Git or Code), the screens, the branches to switch to, the open repositories, and
 * the functions of the scan by name (picked: its calls). The toolbar's search box stays for filtering the code views.
 */
const open = ref(false)
const term = ref('')
const router = useRouter()
const { index, visible, select } = useCode()
const { refs, current } = useRefs()
const { info, chooseRoot } = useRepositories()
const network = useNetwork()
const refActions = useRefActions()
const undo = useUndo()
const { operation } = useOperation()
const { switchTo } = useMode()
const pins = usePins()
const toast = useToast()

async function openTerminal(root: string): Promise<void> {
  try {
    await haunt.terminalTab(root)
  } catch (error) {
    toast.add({ title: 'Could not open a terminal', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}

defineShortcuts({ meta_k: () => { open.value = !open.value } })
watch(open, (now) => { if (!now) term.value = '' })

function run(action: () => unknown): void {
  open.value = false
  void action()
}

const functions = computed(() => {
  const at = index.value
  const query = term.value.trim().toLowerCase()
  if (!at || query.length < 2) return []
  const found: { id: string, name: string, path: string, callers: number }[] = []
  for (const [id, item] of at.functions) {
    if (item.fn.anonymous || !visible(item.file) || !item.fn.name.toLowerCase().includes(query)) continue
    found.push({ id, name: item.fn.name, path: item.file.path, callers: at.callers.get(id)?.length ?? 0 })
  }
  // Names that start with what is typed first, then the most called.
  return found
    .sort((a, b) => Number(b.name.toLowerCase().startsWith(query)) - Number(a.name.toLowerCase().startsWith(query)) || b.callers - a.callers)
    .slice(0, 15)
})

const groups = computed(() => {
  const branch = current.value
  const tabs = (info.value?.tabs ?? []).filter(tab => tab !== info.value?.root)
  const sha = /^[0-9a-f]{7,40}$/i.test(term.value.trim()) ? term.value.trim() : null
  return [
    ...sha
      ? [{ id: 'commit', label: 'Commit', ignoreFilter: true, items: [{ label: `Go to commit ${sha}`, icon: 'i-lucide-git-commit-vertical', onSelect: () => run(() => router.push({ path: '/commits', query: { commit: sha } })) }] }]
      : [],
    {
      id: 'actions',
      label: 'Actions',
      items: [
        ...undo.last.value ? [{ label: `Undo: ${undo.last.value.title}`, suffix: undo.last.value.undo, icon: 'i-lucide-undo-2', onSelect: () => run(() => undo.undo()) }] : [],
        { label: 'Fetch', icon: 'i-lucide-refresh-cw', onSelect: () => run(() => network.fetch()) },
        ...info.value?.root ? [{ label: 'Open a terminal here', icon: 'i-lucide-square-terminal', onSelect: () => run(() => openTerminal(info.value!.root!)) }] : [],
        ...network.trusted.value && branch?.upstream ? [{ label: `Pull ${branch.upstream}`, icon: 'i-lucide-arrow-down-to-line', onSelect: () => run(() => network.pull()) }] : [],
        ...branch?.behind ? [{ label: 'Incoming changes', suffix: 'what a pull would bring in', icon: 'i-lucide-eye', onSelect: () => run(() => router.push('/incoming')) }] : [],
        ...network.trusted.value && branch ? [{ label: `Push ${branch.name}`, icon: 'i-lucide-arrow-up-from-line', onSelect: () => run(() => network.push()) }] : [],
        { label: 'New branch…', icon: 'i-lucide-git-branch-plus', onSelect: () => run(() => refActions.newBranch()) },
        { label: 'Stash local changes…', icon: 'i-lucide-archive', onSelect: () => run(() => refActions.stash()) },
        { label: 'Git mode', suffix: 'history and branches', icon: 'i-lucide-git-branch', kbds: ['meta', '1'], onSelect: () => run(() => switchTo('git')) },
        { label: 'Code mode', suffix: 'layers, canvas, calls, imports', icon: 'i-lucide-braces', kbds: ['meta', '2'], onSelect: () => run(() => switchTo('code')) }
      ]
    },
    {
      id: 'screens',
      label: 'Screens',
      items: [
        { label: 'Local changes', icon: 'i-lucide-pencil-line', to: '/local' },
        { label: 'All commits', icon: 'i-lucide-git-commit-vertical', to: '/commits' },
        { label: 'Pull requests', icon: 'i-lucide-git-pull-request', to: '/pulls' },
        { label: 'Bisect', suffix: 'find the commit that broke something', icon: 'i-lucide-crosshair', to: '/bisect' },
        { label: 'Release notes', suffix: 'between two tags', icon: 'i-lucide-scroll-text', to: '/release' },
        { label: 'Apply patches', icon: 'i-lucide-file-diff', to: '/patches' },
        { label: 'Health', suffix: 'size, large files, old branches', icon: 'i-lucide-heart-pulse', to: '/health' },
        ...branch ? [{ label: `Review ${branch.name}`, suffix: 'function by function', icon: 'i-lucide-list-checks', to: '/review' }] : [],
        ...operation.value ? [{ label: `Conflicts: ${operation.value.title}`, icon: 'i-lucide-git-merge', to: '/conflicts' }] : [],
        { label: 'Layers', icon: 'i-lucide-layers', to: '/layers' },
        { label: 'Canvas', icon: 'i-lucide-boxes', to: '/canvas' },
        { label: 'Calls', icon: 'i-lucide-git-fork', to: '/calls' },
        { label: 'Imports', icon: 'i-lucide-network', to: '/imports' },
        { label: 'Unused functions', icon: 'i-lucide-ghost', to: '/unused' },
        { label: 'Routes', suffix: 'the HTTP routes in the code', icon: 'i-lucide-route', to: '/routes' }
      ].map(item => ({ ...item, onSelect: () => run(() => router.push(item.to)) }))
    },
    {
      id: 'branches',
      label: 'Switch to a branch',
      items: (refs.value?.branches ?? []).filter(item => !item.current).map(item => ({
        label: item.name,
        suffix: item.subject,
        icon: 'i-lucide-git-branch',
        onSelect: () => run(() => refActions.switchTo(item.name))
      }))
    },
    {
      id: 'tabs',
      label: 'Open repositories',
      items: tabs.map(tab => ({ label: tab.split(/[\\/]/).pop() ?? tab, suffix: tab, icon: 'i-lucide-folder-git-2', onSelect: () => run(() => chooseRoot(tab)) }))
    },
    {
      id: 'pinned',
      label: 'Pinned functions',
      items: pins.pinned.value.filter(pin => pin.found).map(pin => ({
        label: pin.name,
        suffix: tailOf(pin.path),
        icon: 'i-lucide-pin',
        onSelect: () => run(() => {
          select(pin.found!.fn.id)
          return router.push({ path: '/calls', query: { center: pin.found!.fn.id, fn: pin.found!.fn.id } })
        })
      }))
    },
    {
      id: 'functions',
      label: 'Functions',
      // Searched here, not by the palette: there are thousands.
      ignoreFilter: true,
      items: functions.value.map(fn => ({
        label: fn.name,
        suffix: `${tailOf(fn.path)} · called from ${fn.callers}`,
        icon: 'i-lucide-square-function',
        onSelect: () => run(() => {
          select(fn.id)
          return router.push({ path: '/calls', query: { center: fn.id, fn: fn.id } })
        })
      }))
    }
  ].filter(group => group.items.length)
})
</script>

<template>
  <UModal v-model:open="open" :ui="{ content: 'max-w-2xl' }" title="Command palette" description="Search for an action, a screen, a branch, a repository or a function.">
    <template #content>
      <UCommandPalette
        v-model:search-term="term"
        :groups="groups"
        placeholder="Type an action, a screen, a branch or a function…"
        close
        class="h-[28rem]"
        @update:open="(value: boolean) => { open = value }"
      />
    </template>
  </UModal>
</template>
