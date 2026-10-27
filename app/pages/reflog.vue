<script setup lang="ts">
import type { Reflog, ReflogEntry } from '~/bindings'

/*
 * The reflog (src-tauri/src/git/reflog.rs): where HEAD, or a branch, pointed before, newest first, and what moved it.
 * A commit that only the reflog still keeps (a reset, a rebase or an amend left it behind, or its branch was deleted)
 * is marked lost; from any entry, see the commit, put a new branch on it, reset the current branch to it or
 * cherry-pick it. Undo goes back one step only; this goes back as far as git keeps (90 days by default).
 */
const route = useRoute()
const { repo, gitChanged } = useRepositories()
const { refs, current } = useRefs()
const { times } = usePreferences()
const failure = useFailure()
const refActions = useRefActions()
const operation = useOperation()

const reference = ref(typeof route.query.ref === 'string' ? route.query.ref : 'HEAD')
const references = computed(() => [{ value: 'HEAD', label: 'HEAD' }, ...(refs.value?.branches ?? []).map(branch => ({ value: branch.name, label: branch.name }))])
const onlyLost = ref(false)

const reflog = shallowRef<Reflog | null>(null)
// A slow answer for the tab or branch before lands nowhere.
const ask = latest(repo)
const { loading } = ask
async function load(): Promise<void> {
  if (!repo.value) return
  try {
    const found = await ask.run(at => haunt.reflog(at, reference.value))
    if (found !== undefined) reflog.value = found
  } catch (error) {
    reflog.value = null
    failure.failed(t('pages.reflog.readFailed'), error)
  }
}
watch([repo, reference], () => {
  reflog.value = null
  void load()
}, { immediate: true })
// Every move of HEAD or a branch adds an entry.
watch(gitChanged('head', 'refs'), () => void load())
// A branch that is gone (deleted, renamed) leaves HEAD's reflog.
watch(references, (now) => {
  if (!now.some(item => item.value === reference.value)) reference.value = 'HEAD'
})

const shown = computed(() => (reflog.value?.entries ?? []).filter(entry => !onlyLost.value || entry.lost))
const lostCount = computed(() => new Set((reflog.value?.entries ?? []).filter(entry => entry.lost).map(entry => entry.sha)).size)

/** git's word for what moved it, in the window's language where Haunt knows it. */
const KNOWN = ['commit', 'commit (initial)', 'commit (amend)', 'commit (merge)', 'checkout', 'reset', 'merge', 'pull', 'rebase', 'cherry-pick', 'revert', 'branch', 'clone', 'fetch', 'stash', 'am'] as const
function actionLabel(entry: ReflogEntry): string {
  const action = entry.action.toLowerCase()
  const known = KNOWN.find(word => action === word) ?? KNOWN.find(word => action.startsWith(`${word} `))
  return known ? t(`pages.reflog.action.${known.replace(/[ ()-]+/g, '_').replace(/_$/, '')}`) : entry.action
}
const actionIcon = (entry: ReflogEntry) => {
  const action = entry.action.toLowerCase()
  if (action.startsWith('reset')) return 'i-lucide-history'
  if (action.startsWith('checkout')) return 'i-lucide-arrow-right-left'
  if (action.startsWith('rebase')) return 'i-lucide-git-pull-request-arrow'
  if (action.startsWith('merge') || action.startsWith('pull')) return 'i-lucide-git-merge'
  if (action.startsWith('cherry-pick')) return 'i-lucide-cherry'
  if (action.startsWith('branch') || action.startsWith('clone')) return 'i-lucide-git-branch-plus'
  return 'i-lucide-git-commit-horizontal'
}

const when = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleString(locale(), { dateStyle: 'short', timeStyle: 'short' })
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return t('pages.reflog.minutes', { count: Math.max(1, minutes) })
  if (minutes < 60 * 24) return t('pages.reflog.hours', { count: Math.floor(minutes / 60) })
  return t('pages.reflog.days', { count: Math.floor(minutes / 60 / 24) })
}

function menu(entry: ReflogEntry) {
  const at = { rev: entry.sha, label: entry.selector }
  return [
    [{ label: t('pages.reflog.showCommit'), icon: 'i-lucide-eye', onSelect: () => navigateTo({ path: '/commits', query: { commit: entry.sha } }) }],
    [
      { label: t('pages.reflog.newBranch'), icon: 'i-lucide-git-branch-plus', onSelect: () => refActions.newBranch(at) },
      { label: current.value ? t('pages.reflog.reset', { branch: current.value.name }) : t('pages.reflog.resetCurrent'), icon: 'i-lucide-history', onSelect: () => operation.reset(entry.sha, entry.selector) },
      { label: current.value ? t('pages.reflog.cherryPick', { branch: current.value.name }) : t('pages.reflog.cherryPickCurrent'), icon: 'i-lucide-cherry', onSelect: () => operation.cherryPick(entry.sha, entry.subject) }
    ],
    [
      { label: t('pages.reflog.copySha'), icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(entry.sha) },
      { label: t('pages.reflog.copySelector'), icon: 'i-lucide-copy', onSelect: () => navigator.clipboard?.writeText(entry.selector) }
    ]
  ]
}
</script>

<template>
  <div class="mx-auto grid w-full max-w-4xl content-start gap-5 px-4 py-8">
    <div class="flex flex-wrap items-start gap-3">
      <div class="grid flex-1 gap-1">
        <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.reflog.title') }}</h1>
        <p class="text-sm text-muted">{{ t('pages.reflog.intro') }}</p>
      </div>
      <UButton :label="t('pages.reflog.readAgain')" icon="i-lucide-refresh-cw" size="sm" color="neutral" variant="ghost" :loading="loading" @click="load" />
    </div>

    <div class="flex flex-wrap items-center gap-3">
      <USelectMenu v-model="reference" :items="references" value-key="value" icon="i-lucide-git-branch" class="w-64" :aria-label="t('pages.reflog.whose')" />
      <USwitch v-model="onlyLost" :label="t('pages.reflog.onlyLost')" :disabled="!lostCount" />
      <span v-if="reflog" class="ml-auto text-xs text-muted">{{ t('pages.reflog.lostCount', { count: lostCount }) }}</span>
    </div>

    <p v-if="loading && !reflog" class="text-sm text-muted">{{ t('pages.reflog.reading') }}</p>
    <p v-else-if="reflog && !shown.length" class="text-sm text-muted">{{ onlyLost ? t('pages.reflog.noLost') : t('pages.reflog.empty') }}</p>

    <ol v-if="shown.length" class="grid gap-0.5">
      <li v-for="entry in shown" :key="entry.selector">
        <UContextMenu :items="menu(entry)">
          <div class="group grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 rounded-md px-2 py-1.5 hover:bg-elevated/60">
            <UIcon :name="actionIcon(entry)" class="size-4 text-muted" />
            <div class="grid min-w-0">
              <div class="flex min-w-0 items-center gap-2 text-sm">
                <span class="shrink-0 font-medium text-highlighted">{{ actionLabel(entry) }}</span>
                <span class="truncate text-muted" :title="entry.message">{{ entry.message }}</span>
              </div>
              <div class="flex min-w-0 items-center gap-2 text-label text-muted">
                <span class="font-mono">{{ entry.selector }}</span>
                <span class="font-mono">{{ entry.sha.slice(0, 7) }}</span>
                <span class="truncate" :title="entry.subject">{{ entry.subject }}</span>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <UBadge v-if="entry.lost" :label="t('pages.reflog.lost')" color="warning" variant="subtle" size="sm" :title="t('pages.reflog.lostHelp')" />
              <span class="text-label text-muted tabular-nums" :title="new Date(entry.time * 1000).toLocaleString(locale())">{{ when(entry.time) }}</span>
              <UDropdownMenu :items="menu(entry)">
                <UButton icon="i-lucide-ellipsis" size="xs" color="neutral" variant="ghost" class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" :aria-label="t('pages.reflog.actions', { selector: entry.selector })" />
              </UDropdownMenu>
            </div>
          </div>
        </UContextMenu>
      </li>
    </ol>
    <p v-if="reflog?.more" class="text-xs text-muted">{{ t('pages.reflog.more') }}</p>
  </div>
</template>
