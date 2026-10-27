<script setup lang="ts">
import type { Overview, RemoteInfo, Statistics } from '~/bindings'
import { hostName, issuesLink, pullsLink } from '~~/shared/weblinks'

/*
 * One repository at a glance, open in a tab or not (git/overview.rs reads it by its path, running nothing of its own):
 * its uncommitted files, how many commits and its first and last, its remotes with their pages on the web, its
 * branches, tags and latest commits; who committed how much and when (Statistics, read when asked for); and its
 * README. A folder that is a project but not a git repository just offers to open it.
 */
const props = defineProps<{ path: string }>()
const { chooseRoot, info } = useRepositories()
const { times } = usePreferences()
const toast = useToast()

const overview = shallowRef<Overview | null>(null)
const problem = ref('')
const loading = ref(true)
onMounted(async () => {
  try {
    overview.value = await haunt.repositoryOverview(props.path)
  } catch (error) {
    problem.value = errorMessage(error, t('RepoOverview.notRead'))
  } finally {
    loading.value = false
  }
})
const name = computed(() => overview.value?.name ?? props.path.split(/[\\/]/).filter(Boolean).pop() ?? props.path)
const inTab = computed(() => info.value?.tabs.includes(overview.value?.path ?? props.path) ?? false)

type View = 'overview' | 'statistics' | 'readme'
const view = ref<View>('overview')
const views = computed(() => (['overview', 'statistics', 'readme'] as const).filter(item => item !== 'readme' || overview.value?.readme))

const statistics = shallowRef<Statistics | null>(null)
const statisticsProblem = ref('')
watch(view, async (now) => {
  if (now !== 'statistics' || statistics.value) return
  try {
    statistics.value = await haunt.repositoryStatistics(props.path)
  } catch (error) {
    statisticsProblem.value = errorMessage(error, t('RepoOverview.notRead'))
  }
})
const busiestMonth = computed(() => Math.max(1, ...(statistics.value?.months.map(month => month.commits) ?? [])))
const busiestAuthor = computed(() => Math.max(1, ...(statistics.value?.authors.map(author => author.commits) ?? [])))
const commonestExtension = computed(() => Math.max(1, ...(statistics.value?.extensions.map(item => item.files) ?? [])))
const monthName = (month: string) => {
  const [year, number] = month.split('-').map(Number)
  return new Date(year ?? 1970, (number ?? 1) - 1, 1).toLocaleDateString(undefined, { month: 'short' })
}

const uncommitted = computed(() => {
  const status = overview.value?.status
  if (!status) return []
  const seen = new Set<string>()
  return [...status.staged, ...status.unstaged].filter(file => !seen.has(file.path) && seen.add(file.path))
})

const ago = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return t('RepoOverview.minutes', { count: Math.max(1, minutes) })
  if (minutes < 60 * 24) return t('RepoOverview.hours', { count: Math.floor(minutes / 60) })
  if (minutes < 60 * 24 * 60) return t('RepoOverview.days', { count: Math.floor(minutes / 60 / 24) })
  return new Date(seconds * 1000).toLocaleDateString()
}
const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString()

async function openPage(remote: RemoteInfo, url: string) {
  try {
    await haunt.openRepositoryPage(overview.value?.path ?? props.path, remote.name, url)
  } catch (error) {
    toast.add({ title: t('RepoOverview.pageNotOpened'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
/** A README's link: a page of one of the remotes opens; any other address is copied (Haunt opens only those pages). */
async function followLink(href: string) {
  const remote = overview.value?.remotes.find(item => item.web && (href === item.web.base || href.startsWith(`${item.web.base}/`)))
  if (remote) return openPage(remote, href)
  if (!/^https?:\/\//i.test(href)) return
  try {
    await navigator.clipboard.writeText(href)
    toast.add({ title: t('RepoOverview.linkCopied'), description: href, icon: 'i-lucide-link' })
  } catch {
    toast.add({ title: t('RepoOverview.linkNotCopied'), description: href, color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function reveal() {
  try {
    await haunt.revealTab(overview.value?.path ?? props.path)
  } catch (error) {
    toast.add({ title: t('RootPicker.folderNotOpened'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
const opening = ref(false)
async function open() {
  opening.value = true
  const failed = await chooseRoot(overview.value?.path ?? props.path)
  opening.value = false
  if (failed) toast.add({ title: t('RepoOverview.notOpened'), description: failed, color: 'error', icon: 'i-lucide-circle-alert' })
}
</script>

<template>
  <div class="flex min-h-0 flex-col overflow-auto">
    <header class="flex flex-wrap items-start gap-3 border-b border-default px-6 py-4">
      <div class="grid min-w-0 flex-1 gap-0.5">
        <h2 class="flex min-w-0 items-center gap-2 text-lg font-semibold text-highlighted">
          <span class="truncate">{{ name }}</span>
          <UBadge v-if="overview?.branch" :label="overview.branch" icon="i-lucide-git-branch" size="sm" variant="subtle" color="neutral" class="font-mono" />
          <UBadge v-if="inTab" :label="t('RootPicker.inTab')" size="sm" variant="subtle" color="neutral" />
        </h2>
        <span class="truncate font-mono text-label text-muted" :title="overview?.path ?? path">{{ overview?.path ?? path }}</span>
      </div>
      <div class="flex gap-1.5">
        <UButton icon="i-lucide-folder-open" :label="t('RepoOverview.reveal')" size="sm" color="neutral" variant="outline" @click="reveal" />
        <UButton icon="i-lucide-arrow-up-right" :label="inTab ? t('RepoOverview.show') : t('RepoOverview.open')" size="sm" :loading="opening" @click="open" />
      </div>
    </header>

    <div v-if="loading" class="grid place-items-center p-10"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
    <div v-else-if="!overview" class="grid gap-2 p-6 text-sm text-muted">
      <p>{{ problem }}</p>
      <p>{{ t('RepoOverview.notGitHelp') }}</p>
    </div>
    <template v-else>
      <nav class="flex gap-1 px-6 pt-3" :aria-label="t('RepoOverview.views')">
        <UButton
          v-for="item in views"
          :key="item"
          :label="t(`RepoOverview.view.${item}`)"
          size="sm"
          :color="view === item ? 'primary' : 'neutral'"
          :variant="view === item ? 'soft' : 'ghost'"
          @click="view = item"
        />
      </nav>

      <div v-if="view === 'overview'" class="grid gap-6 p-6 text-sm xl:grid-cols-2">
        <section class="grid content-start gap-2 xl:col-span-2">
          <dl class="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div class="grid gap-0.5 rounded-md border border-default p-3">
              <dt class="text-label text-muted">{{ t('RepoOverview.uncommitted') }}</dt>
              <dd class="text-lg font-semibold tabular-nums" :class="uncommitted.length ? 'text-warning' : 'text-highlighted'">{{ uncommitted.length }}</dd>
            </div>
            <div class="grid gap-0.5 rounded-md border border-default p-3">
              <dt class="text-label text-muted">{{ t('RepoOverview.commits') }}</dt>
              <dd class="text-lg font-semibold text-highlighted tabular-nums">{{ overview.commits.toLocaleString() }}</dd>
            </div>
            <div class="grid min-w-0 gap-0.5 rounded-md border border-default p-3">
              <dt class="text-label text-muted">{{ t('RepoOverview.first') }}</dt>
              <dd v-if="overview.first" class="grid min-w-0">
                <span class="font-semibold text-highlighted">{{ day(overview.first.time) }}</span>
                <span class="truncate text-label text-muted" :title="overview.first.subject">{{ overview.first.author }}</span>
              </dd>
              <dd v-else class="text-muted">{{ t('RepoOverview.none') }}</dd>
            </div>
            <div class="grid min-w-0 gap-0.5 rounded-md border border-default p-3">
              <dt class="text-label text-muted">{{ t('RepoOverview.last') }}</dt>
              <dd v-if="overview.last" class="grid min-w-0">
                <span class="font-semibold text-highlighted" :title="day(overview.last.time)">{{ ago(overview.last.time) }}</span>
                <span class="truncate text-label text-muted" :title="overview.last.subject">{{ overview.last.subject }}</span>
              </dd>
              <dd v-else class="text-muted">{{ t('RepoOverview.none') }}</dd>
            </div>
          </dl>
          <ul v-if="uncommitted.length" class="grid gap-0.5 font-mono text-xs">
            <li v-for="file in uncommitted.slice(0, 12)" :key="file.path" class="flex min-w-0 gap-2">
              <span class="w-4 shrink-0 text-warning">{{ file.untracked ? '?' : file.conflicted ? '!' : t(`RepoOverview.status.${file.status}`) }}</span>
              <span class="truncate text-muted" :title="file.path">{{ file.path }}</span>
            </li>
            <li v-if="uncommitted.length > 12" class="text-muted">{{ t('RepoOverview.more', { count: uncommitted.length - 12 }) }}</li>
          </ul>
        </section>

        <section class="grid content-start gap-2">
          <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.remotes') }}</h3>
          <p v-if="!overview.remotes.length" class="text-muted">{{ t('RepoOverview.noRemotes') }}</p>
          <div v-for="remote in overview.remotes" :key="remote.name" class="grid gap-1.5 rounded-md border border-default p-3">
            <div class="flex min-w-0 items-center gap-2">
              <UIcon name="i-lucide-cloud" class="size-4 shrink-0 text-muted" />
              <span class="font-medium text-highlighted">{{ remote.name }}</span>
              <span class="truncate font-mono text-label text-muted" :title="remote.url">{{ remote.url }}</span>
            </div>
            <div v-if="remote.web" class="flex flex-wrap gap-1">
              <UButton :label="t('RepoOverview.openOn', { host: hostName(remote.web.kind) })" icon="i-lucide-external-link" size="xs" color="neutral" variant="outline" @click="openPage(remote, remote.web.base)" />
              <UButton :label="t('RepoOverview.issues')" icon="i-lucide-circle-dot" size="xs" color="neutral" variant="ghost" @click="openPage(remote, issuesLink(remote.web))" />
              <UButton :label="remote.web.kind === 'gitlab' ? t('RepoOverview.mergeRequests') : t('RepoOverview.pulls')" icon="i-lucide-git-pull-request" size="xs" color="neutral" variant="ghost" @click="openPage(remote, pullsLink(remote.web))" />
            </div>
          </div>
        </section>

        <section class="grid content-start gap-2">
          <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.branches') }} <span class="font-normal tabular-nums">{{ overview.branches.length }}</span></h3>
          <ul class="grid gap-0.5">
            <li v-for="branch in overview.branches.slice(0, 15)" :key="branch.name" class="flex min-w-0 items-center gap-2">
              <UIcon :name="branch.current ? 'i-lucide-circle-dot' : 'i-lucide-git-branch'" class="size-3.5 shrink-0" :class="branch.current ? 'text-primary' : 'text-muted'" />
              <span class="truncate font-mono text-xs" :class="branch.current ? 'font-semibold text-highlighted' : ''">{{ branch.name }}</span>
              <span v-if="branch.ahead || branch.behind" class="shrink-0 font-mono text-label text-muted">
                <template v-if="branch.ahead">↑{{ branch.ahead }}</template>
                <template v-if="branch.behind"> ↓{{ branch.behind }}</template>
              </span>
              <span class="ml-auto shrink-0 text-label text-muted">{{ ago(branch.time) }}</span>
            </li>
            <li v-if="overview.branches.length > 15" class="text-xs text-muted">{{ t('RepoOverview.more', { count: overview.branches.length - 15 }) }}</li>
          </ul>
        </section>

        <section class="grid content-start gap-2">
          <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.latest') }}</h3>
          <p v-if="!overview.latest.length" class="text-muted">{{ t('RepoOverview.noCommits') }}</p>
          <ul class="grid gap-1">
            <li v-for="commit in overview.latest" :key="commit.sha" class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-x-3">
              <span class="truncate" :title="commit.subject">{{ commit.subject }}</span>
              <span class="text-label text-muted">{{ ago(commit.time) }}</span>
              <span class="truncate text-label text-muted">{{ commit.author }}</span>
              <span class="font-mono text-label text-muted">{{ commit.sha.slice(0, 7) }}</span>
            </li>
          </ul>
        </section>

        <section class="grid content-start gap-2">
          <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.tags') }} <span class="font-normal tabular-nums">{{ overview.tagCount }}</span></h3>
          <p v-if="!overview.tags.length" class="text-muted">{{ t('RepoOverview.noTags') }}</p>
          <ul class="grid gap-0.5">
            <li v-for="tag in overview.tags.slice(0, 15)" :key="tag.name" class="flex min-w-0 items-center gap-2">
              <UIcon name="i-lucide-tag" class="size-3.5 shrink-0 text-muted" />
              <span class="truncate font-mono text-xs">{{ tag.name }}</span>
              <span class="ml-auto shrink-0 text-label text-muted">{{ day(tag.time) }}</span>
            </li>
            <li v-if="overview.tagCount > 15" class="text-xs text-muted">{{ t('RepoOverview.more', { count: overview.tagCount - 15 }) }}</li>
          </ul>
        </section>
      </div>

      <div v-else-if="view === 'statistics'" class="grid gap-6 p-6 text-sm">
        <p v-if="statisticsProblem" class="text-error">{{ statisticsProblem }}</p>
        <div v-else-if="!statistics" class="grid place-items-center p-6"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
        <template v-else>
          <section class="grid gap-2">
            <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.perMonth') }}</h3>
            <div class="flex h-32 items-end gap-1.5" role="img" :aria-label="t('RepoOverview.perMonth')">
              <div v-for="month in statistics.months" :key="month.month" class="grid h-full flex-1 grid-rows-[minmax(0,1fr)_auto] gap-1" :title="t('RepoOverview.commitsIn', { count: month.commits, month: monthName(month.month) })">
                <div class="flex items-end">
                  <div class="w-full rounded-t-sm bg-primary/70" :style="{ height: `${(month.commits / busiestMonth) * 100}%`, minHeight: month.commits ? '2px' : '0' }" />
                </div>
                <span class="text-center text-label text-muted">{{ monthName(month.month) }}</span>
              </div>
            </div>
          </section>
          <div class="grid gap-6 lg:grid-cols-2">
            <section class="grid content-start gap-2">
              <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.authors') }}</h3>
              <ul class="grid gap-1.5">
                <li v-for="author in statistics.authors" :key="author.email || author.name" class="grid gap-0.5">
                  <span class="flex min-w-0 gap-2"><span class="truncate" :title="author.email">{{ author.name }}</span><span class="ml-auto text-muted tabular-nums">{{ author.commits.toLocaleString() }}</span></span>
                  <span class="h-1 rounded-full bg-primary/60" :style="{ width: `${(author.commits / busiestAuthor) * 100}%` }" />
                </li>
              </ul>
            </section>
            <section class="grid content-start gap-2">
              <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('RepoOverview.files', { count: statistics.files }) }}</h3>
              <ul class="grid gap-1.5">
                <li v-for="item in statistics.extensions" :key="item.extension" class="grid gap-0.5">
                  <span class="flex gap-2"><span class="font-mono text-xs">{{ item.extension ? `.${item.extension}` : t('RepoOverview.noExtension') }}</span><span class="ml-auto text-muted tabular-nums">{{ item.files.toLocaleString() }}</span></span>
                  <span class="h-1 rounded-full bg-neutral-400/60" :style="{ width: `${(item.files / commonestExtension) * 100}%` }" />
                </li>
              </ul>
            </section>
          </div>
        </template>
      </div>

      <div v-else-if="overview.readme" class="grid gap-3 p-6">
        <p class="font-mono text-label text-muted">{{ overview.readme.name }}</p>
        <MarkdownView :text="overview.readme.text" @link="followLink" />
        <p v-if="overview.readme.cut" class="text-xs text-muted">{{ t('RepoOverview.readmeCut') }}</p>
      </div>
    </template>
  </div>
</template>
