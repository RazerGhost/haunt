<script setup lang="ts">
import type { CommitInfo, PullRequest } from '~/bindings'

/*
 * Pull requests, reviewed here without switching your branch: fetched as refs from the remote (the newest 30, or one by
 * its number; src-tauri/src/git/write/network.rs), with no account or API, so each is named by its number and its
 * newest commit. A pull request opens with every lens of the commit list (files, functions, what could break and the
 * tests that reach it, the map, the calls) on what it changes since it went apart from the main branch; one of its
 * commits can be looked at alone.
 */
const { repo, gitChanged } = useRepositories()
const route = useRoute()
const router = useRouter()
const failure = useFailure()
const toast = useToast()
const refActions = useRefActions()
const { times } = usePreferences()
// Titles and states from the host's tool, when Settings has it switched on (useHostPulls).
const { byNumber } = useHostPulls()

const pulls = shallowRef<PullRequest[] | null>(null)
// New pull request (PullCreate).
const creating = ref(false)
const problem = ref('')
const fetching = ref(false)
const number = ref('')
const ask = latest(repo)

async function load(): Promise<void> {
  try {
    const found = await ask.run(at => haunt.pullRequests(at))
    if (found !== undefined) pulls.value = found
    problem.value = ''
  } catch (error) {
    problem.value = errorMessage(error, t('pages.pulls.readFailed'))
  }
}
watch([repo, gitChanged('refs')], () => repo.value && void load(), { immediate: true })

async function fetchThem(one: number | null = null): Promise<void> {
  if (!repo.value || fetching.value) return
  fetching.value = true
  try {
    const done = await haunt.fetchPullRequests(repo.value, null, one)
    toast.add({
      title: one ? t('pages.pulls.fetchedOne', { number: one }) : t('pages.pulls.fetched', { count: done.fetched, remote: done.remote }),
      description: one || done.total <= done.fetched ? undefined : t('pages.pulls.newestOf', { count: done.fetched, total: done.total }),
      icon: 'i-lucide-git-pull-request',
      color: 'success'
    })
    await load()
    if (one) choose(pulls.value?.find(pull => pull.number === one) ?? null)
  } catch (error) {
    failure.failed(t('pages.pulls.notFetched'), error, t('pages.pulls.fetchFailed'))
  } finally {
    fetching.value = false
  }
}
function fetchNumber(): void {
  const wanted = Number(number.value.replace('#', '').trim())
  if (Number.isInteger(wanted) && wanted > 0) {
    number.value = ''
    void fetchThem(wanted)
  }
}

const keyOf = (pull: PullRequest) => `${pull.remote}/${pull.number}`
const chosen = computed(() => pulls.value?.find(pull => keyOf(pull) === route.query.pr) ?? null)
function choose(pull: PullRequest | null): void {
  void router.replace({ query: pull ? { pr: keyOf(pull) } : {} })
}

const commits = shallowRef<CommitInfo[]>([])
const askCommits = latest(repo)
watch(chosen, async (pull) => {
  commits.value = []
  if (!pull) return
  const found = await askCommits.run(at => haunt.pullCommits(at, pull.base, pull.sha)).catch(() => [])
  if (found !== undefined) commits.value = found
}, { immediate: true })
/** One of its commits looked at alone; the whole pull request when none. */
const alone = computed(() => (typeof route.query.commit === 'string' ? route.query.commit : null))
function look(sha: string | null): void {
  void router.replace({ query: { ...route.query, commit: sha ?? undefined } })
}

const when = (seconds: number) => times.value === 'date'
  ? new Date(seconds * 1000).toLocaleString(locale(), { dateStyle: 'short', timeStyle: 'short' })
  : new Date(seconds * 1000).toLocaleDateString(locale())
</script>

<template>
  <div class="grid h-full min-h-0 grid-cols-[18rem_minmax(0,1fr)]">
    <aside class="flex min-h-0 flex-col border-r border-default">
      <div class="grid gap-2 border-b border-default p-3">
        <UButton :label="pulls?.length ? t('pages.pulls.fetchAgain') : t('pages.pulls.fetchPulls')" icon="i-lucide-cloud-download" size="sm" :loading="fetching" block @click="fetchThem()" />
        <UButton :label="t('pages.pulls.newPull')" icon="i-lucide-git-pull-request-create" size="sm" color="neutral" variant="outline" block @click="creating = true" />
        <form class="flex gap-1.5" @submit.prevent="fetchNumber">
          <UInput v-model="number" size="xs" :placeholder="t('pages.pulls.numberPlaceholder')" class="flex-1" :aria-label="t('pages.pulls.number')" />
          <UButton type="submit" :label="t('pages.pulls.fetch')" size="xs" color="neutral" variant="outline" :disabled="!number.trim() || fetching" />
        </form>
        <p class="text-label text-muted">{{ t('pages.pulls.note') }}</p>
      </div>
      <UAlert v-if="problem" class="m-3" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
      <p v-else-if="pulls && !pulls.length" class="p-4 text-sm text-muted">{{ t('pages.pulls.none') }}</p>
      <ul v-else class="min-h-0 flex-1 overflow-auto p-1">
        <li v-for="pull in pulls ?? []" :key="keyOf(pull)">
          <button
            type="button"
            class="grid w-full gap-0.5 rounded px-2 py-1.5 text-left"
            :class="chosen && keyOf(chosen) === keyOf(pull) ? 'bg-elevated' : 'hover:bg-elevated/60'"
            @click="choose(pull)"
          >
            <span class="flex items-center gap-1.5 text-ui">
              <span class="font-mono font-semibold text-highlighted">#{{ pull.number }}</span>
              <span class="min-w-0 flex-1 truncate">{{ byNumber.get(pull.number)?.title ?? pull.subject }}</span>
              <UBadge v-if="byNumber.get(pull.number)" :label="pullState(byNumber.get(pull.number)!)" size="sm" color="neutral" variant="subtle" :title="describePull(byNumber.get(pull.number)!)" />
            </span>
            <span class="truncate text-label text-muted">{{ pull.author }} · {{ when(pull.time) }} · {{ t('pages.pulls.commits', { count: pull.commits }) }}<template v-if="pull.remote !== 'origin'"> · {{ pull.remote }}</template></span>
          </button>
        </li>
      </ul>
    </aside>

    <section v-if="chosen" class="flex min-h-0 flex-col">
      <div class="flex flex-wrap items-center gap-2 border-b border-default px-4 py-2">
        <div class="min-w-0 flex-1">
          <h1 class="truncate text-sm font-semibold text-highlighted">#{{ chosen.number }} · {{ chosen.subject }}</h1>
          <p class="truncate text-xs text-muted">
            {{ chosen.baseLabel ? t('pages.pulls.compared', { count: chosen.commits, author: chosen.author, base: chosen.baseLabel }) : t('pages.pulls.noBase', { count: chosen.commits, author: chosen.author }) }}
          </p>
        </div>
        <UButton :label="t('pages.pulls.inCommitList')" icon="i-lucide-git-commit-vertical" size="xs" color="neutral" variant="outline" :to="{ path: '/commits', query: { from: chosen.reference, commit: chosen.sha } }" />
        <UButton :label="t('pages.pulls.newBranch')" icon="i-lucide-git-branch-plus" size="xs" color="neutral" variant="outline" @click="refActions.newBranch({ rev: chosen.sha, label: `#${chosen.number}` })" />
      </div>
      <div v-if="commits.length > 1" class="flex items-center gap-1 overflow-x-auto border-b border-default px-4 py-1.5">
        <UButton :label="t('pages.pulls.allChanges')" size="xs" :color="alone ? 'neutral' : 'primary'" :variant="alone ? 'ghost' : 'soft'" @click="look(null)" />
        <UButton
          v-for="commit in commits"
          :key="commit.sha"
          :label="commit.subject"
          size="xs"
          :color="alone === commit.sha ? 'primary' : 'neutral'"
          :variant="alone === commit.sha ? 'soft' : 'ghost'"
          class="max-w-56 shrink-0"
          :ui="{ label: 'truncate' }"
          :title="`${commit.sha.slice(0, 7)} · ${commit.author} · ${commit.subject}`"
          @click="look(commit.sha)"
        />
      </div>
      <CommitDetails
        :key="`${chosen.sha}:${alone ?? ''}`"
        :sha="alone ?? chosen.sha"
        :compare="alone ? null : chosen.base"
        alone
        class="min-h-0 flex-1"
        @select="sha => router.push({ path: '/commits', query: { commit: sha } })"
      />
    </section>
    <div v-else class="grid place-items-center p-8 text-center text-sm text-muted">
      <p>{{ pulls?.length ? t('pages.pulls.pick') : t('pages.pulls.fetchToReview') }}</p>
    </div>
    <PullCreate v-model:open="creating" />
  </div>
</template>
