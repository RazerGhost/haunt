<script setup lang="ts">
import type { CommitInfo, RebaseAction } from '~/bindings'

/*
 * Interactive rebase of the current branch onto a commit (git/write/rebase.rs): its commits after that one, oldest at
 * the top, to put in a new order and to keep, reword, squash into the one above (both messages kept), fix up into it
 * (its message only) or drop. Nothing is written until Rebase; a rebase that stops on conflicts goes to the Conflicts
 * screen, which continues or aborts it.
 */
const { repo } = useRepositories()
const { current } = useRefs()
const route = useRoute()
const router = useRouter()
const dialog = useDialog()
const toast = useToast()
const failure = useFailure()

const onto = computed(() => (typeof route.query.onto === 'string' ? route.query.onto : ''))
interface Row { commit: CommitInfo, action: RebaseAction, message: string }
const rows = ref<Row[]>([])
const problem = ref('')
const loading = ref(false)
const running = ref(false)
const pushed = ref<string[]>([])
/** The order as read, to tell whether anything changed. */
const original = ref<string[]>([])
/** True when the commits would land somewhere new (onto a branch that moved on), so a rebase with no edits still does something. */
const moves = ref(false)

watch([onto, repo], async () => {
  rows.value = []
  moves.value = false
  problem.value = ''
  if (!onto.value || !repo.value) return
  loading.value = true
  try {
    const plan = await haunt.rebasePlan(repo.value, onto.value)
    rows.value = plan.map(commit => ({ commit, action: 'pick', message: commit.subject }))
    original.value = plan.map(commit => commit.sha)
    if (plan.length) {
      const [target, first] = await Promise.all([haunt.commit(repo.value, onto.value), haunt.commit(repo.value, plan[0]!.sha)])
      moves.value = first.parents[0] !== target.sha
    }
    pushed.value = plan.length ? await haunt.onRemotes(repo.value, 'HEAD').catch(() => []) : []
  } catch (error) {
    problem.value = errorMessage(error, t('pages.rebase.notRead'))
  } finally {
    loading.value = false
  }
}, { immediate: true })

const ACTIONS = computed(() => (['pick', 'reword', 'squash', 'fixup', 'drop'] as RebaseAction[]).map(value => ({ value, label: t(`pages.rebase.action.${value}`) })))
function move(at: number, by: number): void {
  const next = at + by
  if (next < 0 || next >= rows.value.length) return
  const list = [...rows.value]
  const moved = list[at]!
  list[at] = list[next]!
  list[next] = moved
  rows.value = list
}
const changed = computed(() => moves.value || rows.value.some((row, at) => row.action !== 'pick' || row.commit.sha !== original.value[at]))
const firstKept = computed(() => rows.value.find(row => row.action !== 'drop'))
const blocked = computed(() => {
  if (!rows.value.length) return t('pages.rebase.noCommits')
  if (!firstKept.value) return t('pages.rebase.allDropped')
  if (firstKept.value.action === 'squash' || firstKept.value.action === 'fixup') return t('pages.rebase.nothingAbove')
  if (rows.value.some(row => row.action === 'reword' && !row.message.trim())) return t('pages.rebase.needsMessage')
  return ''
})

async function start(): Promise<void> {
  if (!repo.value || blocked.value) return
  const dropped = rows.value.filter(row => row.action === 'drop').length
  const ok = await dialog.confirm({
    title: t('pages.rebase.confirmTitle', { branch: current.value?.name ?? t('pages.rebase.thisBranch') }),
    description: [
      moves.value ? t('pages.rebase.rewrittenOnto', { count: rows.value.length, onto: onto.value }) : t('pages.rebase.rewrittenOrder', { count: rows.value.length }),
      dropped ? t('pages.rebase.dropped', { count: dropped }) : '',
      pushed.value.length ? t('pages.rebase.pushed', { remotes: pushed.value.join(', ') }) : ''
    ].filter(Boolean).join(' '),
    confirm: t('pages.rebase.rebase'),
    danger: dropped > 0 || pushed.value.length > 0
  })
  if (!ok) return
  running.value = true
  try {
    const steps = rows.value.map(row => ({ sha: row.commit.sha, action: row.action, message: row.action === 'reword' ? row.message : null }))
    const outcome = await haunt.rebase(repo.value, onto.value, steps)
    if (outcome === 'conflicts') {
      toast.add({ title: t('pages.rebase.stopped'), description: t('pages.rebase.stoppedHelp'), color: 'warning', icon: 'i-lucide-git-merge' })
      void router.push('/conflicts')
    } else {
      toast.add({ title: t('pages.rebase.rebased'), icon: 'i-lucide-check' })
      void router.push('/commits')
    }
  } catch (error) {
    failure.failed(t('pages.rebase.notRebased'), error)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <div class="mx-auto grid max-w-4xl content-start gap-4 p-5">
    <div>
      <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.rebase.title') }}</h1>
      <p class="text-ui text-muted">{{ t('pages.rebase.intro', { branch: current?.name ?? t('pages.rebase.thisBranchStart'), onto: onto.slice(0, 7) }) }}</p>
    </div>
    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <UAlert v-else-if="pushed.length" color="warning" variant="subtle" icon="i-lucide-triangle-alert" :description="t('pages.rebase.pushedAlert', { remotes: pushed.join(', ') })" />
    <div v-if="loading" class="grid justify-items-center py-6"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
    <ol class="grid gap-1.5">
      <li v-for="(row, at) in rows" :key="row.commit.sha" class="grid gap-2 rounded-md border border-default p-2" :class="row.action === 'drop' ? 'opacity-50' : ''">
        <div class="flex items-center gap-2">
          <div class="flex flex-col">
            <UButton icon="i-lucide-chevron-up" size="xs" color="neutral" variant="ghost" :disabled="at === 0" :aria-label="t('pages.rebase.moveUp')" @click="move(at, -1)" />
            <UButton icon="i-lucide-chevron-down" size="xs" color="neutral" variant="ghost" :disabled="at === rows.length - 1" :aria-label="t('pages.rebase.moveDown')" @click="move(at, 1)" />
          </div>
          <span class="font-mono text-label text-muted">{{ row.commit.sha.slice(0, 7) }}</span>
          <span class="min-w-0 flex-1 truncate text-ui" :class="row.action === 'drop' ? 'line-through' : 'text-highlighted'">{{ row.commit.subject }}</span>
          <USelect v-model="row.action" :items="ACTIONS" size="xs" class="w-56" />
        </div>
        <UTextarea v-if="row.action === 'reword'" v-model="row.message" :rows="2" autoresize class="w-full" :placeholder="t('pages.rebase.newMessage')" />
      </li>
    </ol>
    <div class="flex items-center gap-3">
      <p class="min-w-0 flex-1 text-ui text-muted">{{ blocked || (changed ? '' : t('pages.rebase.nothingChanged')) }}</p>
      <UButton :label="t('common.cancel')" color="neutral" variant="ghost" to="/commits" />
      <UButton :label="t('pages.rebase.rebase')" icon="i-lucide-git-pull-request-arrow" :disabled="!!blocked || !changed || running" :loading="running" @click="start" />
    </div>
  </div>
</template>
