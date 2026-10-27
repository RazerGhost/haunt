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
    toast.add({ title: t('AppPalette.terminalFailed'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
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
      ? [{ id: 'commit', label: t('AppPalette.commit'), ignoreFilter: true, items: [{ label: t('AppPalette.goToCommit', { sha }), icon: 'i-lucide-git-commit-vertical', onSelect: () => run(() => router.push({ path: '/commits', query: { commit: sha } })) }] }]
      : [],
    {
      id: 'actions',
      label: t('AppPalette.actions'),
      items: [
        ...undo.last.value ? [{ label: t('AppPalette.undo', { title: undo.last.value.title }), suffix: undo.last.value.undo, icon: 'i-lucide-undo-2', onSelect: () => run(() => undo.undo()) }] : [],
        { label: t('AppPalette.fetch'), icon: 'i-lucide-refresh-cw', onSelect: () => run(() => network.fetch()) },
        ...info.value?.root ? [{ label: t('AppPalette.terminal'), icon: 'i-lucide-square-terminal', onSelect: () => run(() => openTerminal(info.value!.root!)) }] : [],
        ...network.trusted.value && branch?.upstream ? [{ label: t('AppPalette.pull', { upstream: branch.upstream }), icon: 'i-lucide-arrow-down-to-line', onSelect: () => run(() => network.pull()) }] : [],
        ...branch?.behind ? [{ label: t('AppPalette.incoming'), suffix: t('AppPalette.incomingHelp'), icon: 'i-lucide-eye', onSelect: () => run(() => router.push('/incoming')) }] : [],
        ...network.trusted.value && branch ? [{ label: t('AppPalette.push', { name: branch.name }), icon: 'i-lucide-arrow-up-from-line', onSelect: () => run(() => network.push()) }] : [],
        { label: t('AppPalette.newBranch'), icon: 'i-lucide-git-branch-plus', onSelect: () => run(() => refActions.newBranch()) },
        { label: t('AppPalette.stash'), icon: 'i-lucide-archive', onSelect: () => run(() => refActions.stash()) },
        { label: t('AppPalette.gitMode'), suffix: t('AppPalette.gitModeHelp'), icon: 'i-lucide-git-branch', kbds: ['meta', '1'], onSelect: () => run(() => switchTo('git')) },
        { label: t('AppPalette.codeMode'), suffix: t('AppPalette.codeModeHelp'), icon: 'i-lucide-braces', kbds: ['meta', '2'], onSelect: () => run(() => switchTo('code')) }
      ]
    },
    {
      id: 'screens',
      label: t('AppPalette.screens'),
      items: [
        { label: t('AppPalette.localChanges'), icon: 'i-lucide-pencil-line', to: '/local' },
        { label: t('AppPalette.allCommits'), icon: 'i-lucide-git-commit-vertical', to: '/commits' },
        { label: t('AppPalette.pullRequests'), icon: 'i-lucide-git-pull-request', to: '/pulls' },
        { label: t('AppPalette.bisect'), suffix: t('AppPalette.bisectHelp'), icon: 'i-lucide-crosshair', to: '/bisect' },
        { label: t('AppPalette.reflog'), suffix: t('AppPalette.reflogHelp'), icon: 'i-lucide-footprints', to: '/reflog' },
        { label: t('AppPalette.releaseNotes'), suffix: t('AppPalette.releaseNotesHelp'), icon: 'i-lucide-scroll-text', to: '/release' },
        { label: t('AppPalette.patches'), icon: 'i-lucide-file-diff', to: '/patches' },
        { label: t('AppPalette.health'), suffix: t('AppPalette.healthHelp'), icon: 'i-lucide-heart-pulse', to: '/health' },
        ...branch ? [{ label: t('AppPalette.review', { name: branch.name }), suffix: t('AppPalette.reviewHelp'), icon: 'i-lucide-list-checks', to: '/review' }] : [],
        ...operation.value ? [{ label: t('AppPalette.conflicts', { title: operation.value.title }), icon: 'i-lucide-git-merge', to: '/conflicts' }] : [],
        { label: t('AppPalette.layers'), icon: 'i-lucide-layers', to: '/layers' },
        { label: t('AppPalette.canvas'), icon: 'i-lucide-boxes', to: '/canvas' },
        { label: t('AppPalette.calls'), icon: 'i-lucide-git-fork', to: '/calls' },
        { label: t('AppPalette.imports'), icon: 'i-lucide-network', to: '/imports' },
        { label: t('AppPalette.unused'), icon: 'i-lucide-ghost', to: '/unused' },
        { label: t('AppPalette.routes'), suffix: t('AppPalette.routesHelp'), icon: 'i-lucide-route', to: '/routes' }
      ].map(item => ({ ...item, onSelect: () => run(() => router.push(item.to)) }))
    },
    {
      id: 'branches',
      label: t('AppPalette.branches'),
      items: (refs.value?.branches ?? []).filter(item => !item.current).map(item => ({
        label: item.name,
        suffix: item.subject,
        icon: 'i-lucide-git-branch',
        onSelect: () => run(() => refActions.switchTo(item.name))
      }))
    },
    {
      id: 'tabs',
      label: t('AppPalette.tabs'),
      items: tabs.map(tab => ({ label: tab.split(/[\\/]/).pop() ?? tab, suffix: tab, icon: 'i-lucide-folder-git-2', onSelect: () => run(() => chooseRoot(tab)) }))
    },
    {
      id: 'pinned',
      label: t('AppPalette.pinned'),
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
      label: t('AppPalette.functions'),
      // Searched here, not by the palette: there are thousands.
      ignoreFilter: true,
      items: functions.value.map(fn => ({
        label: fn.name,
        suffix: t('AppPalette.calledFrom', { path: tailOf(fn.path), count: fn.callers }),
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
  <UModal v-model:open="open" :ui="{ content: 'max-w-2xl' }" :title="t('AppPalette.title')" :description="t('AppPalette.description')">
    <template #content>
      <UCommandPalette
        v-model:search-term="term"
        :groups="groups"
        :placeholder="t('AppPalette.placeholder')"
        close
        class="h-[28rem]"
        @update:open="(value: boolean) => { open = value }"
      />
    </template>
  </UModal>
</template>
