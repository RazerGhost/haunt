<script setup lang="ts">
import type { FileVersion } from '~/bindings'

/*
 * A file's history: every commit that touched it, newest first, followed across renames, and the picked commit's change
 * to the file. Pages load as the list is scrolled to its end. Blame shows who last changed each line, at the picked
 * commit or now.
 */
const { repo } = useRepositories()
const route = useRoute()
const router = useRouter()
const { times } = usePreferences()

const path = computed(() => (typeof route.query.path === 'string' ? route.query.path : ''))
const versions = shallowRef<FileVersion[]>([])
const more = ref(false)
const problem = ref('')
const ask = latest(repo)
const { loading } = ask
const PAGE = 100

async function load(fresh: boolean): Promise<void> {
  if (!path.value) return
  problem.value = ''
  try {
    const found = await ask.run(at => haunt.fileHistory(at, path.value, fresh ? 0 : versions.value.length, PAGE))
    if (found === undefined) return
    versions.value = fresh ? found.versions : [...versions.value, ...found.versions]
    more.value = found.more
  } catch (error) {
    problem.value = errorMessage(error, t('pages.history.readError'))
  }
}
watch([path, repo], () => void load(true), { immediate: true })

const picked = computed(() => {
  const sha = typeof route.query.at === 'string' ? route.query.at : null
  return versions.value.find(version => version.commit.sha === sha) ?? versions.value[0] ?? null
})
/** A second version, Ctrl-clicked: the file is compared between the two (older to newer). */
const compared = computed(() => {
  const sha = typeof route.query.with === 'string' ? route.query.with : null
  return sha ? versions.value.find(version => version.commit.sha === sha) ?? null : null
})
const span = computed(() => {
  const one = picked.value
  if (!one) return null
  const two = compared.value
  if (!two || two === one) return { from: one.parent, to: one.commit.sha, path: one.path, oldPath: one.oldPath }
  const [older, newer] = two.commit.time <= one.commit.time ? [two, one] : [one, two]
  return { from: older.commit.sha, to: newer.commit.sha, path: newer.path, oldPath: older.path }
})
function pick(version: FileVersion, event?: MouseEvent): void {
  if ((event?.ctrlKey || event?.metaKey) && picked.value && version !== picked.value) void router.replace({ query: { ...route.query, with: version.commit.sha } })
  else void router.replace({ query: { ...route.query, at: version.commit.sha, with: undefined } })
}

// A filter on the commits loaded: by message or author.
const filter = ref('')
const shownVersions = computed(() => {
  const text = filter.value.trim().toLowerCase()
  return text ? versions.value.filter(version => version.commit.subject.toLowerCase().includes(text) || version.commit.author.toLowerCase().includes(text)) : versions.value
})
function onScroll(event: Event): void {
  const list = event.target as HTMLElement
  if (more.value && !loading.value && list.scrollTop + list.clientHeight > list.scrollHeight - 200) void load(false)
}
const when = (seconds: number) => times.value === 'date'
  ? new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
  : new Date(seconds * 1000).toLocaleDateString()
// A version's badge, `pages.history.status.<status>`; a modified file has none.
const badge = (status: FileVersion['status']) => status === 'modified' ? '' : t(`pages.history.status.${status}`)
</script>

<template>
  <div v-if="!path" class="py-16 text-center text-muted">{{ t('pages.history.pickFile') }}</div>
  <div v-else class="grid h-full grid-cols-[22rem_minmax(0,1fr)]">
    <aside class="flex min-h-0 flex-col border-r border-default">
      <div class="flex items-center gap-2 border-b border-default px-3 py-2">
        <UIcon name="i-lucide-history" class="size-4 shrink-0 text-muted" />
        <span class="min-w-0 flex-1 truncate font-mono text-ui text-highlighted" :title="path">{{ path }}</span>
        <UButton :label="t('pages.history.blame')" icon="i-lucide-user-round-search" size="xs" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path } }" />
      </div>
      <div class="border-b border-default p-1.5">
        <UInput v-model="filter" icon="i-lucide-filter" :placeholder="t('pages.history.filter')" size="xs" class="w-full" :aria-label="t('pages.history.filterLabel')" />
      </div>
      <p v-if="problem" class="p-3 text-ui text-error">{{ problem }}</p>
      <p class="px-3 pt-1.5 text-label text-muted">{{ t('pages.history.compareHint') }}</p>
      <ul class="min-h-0 flex-1 overflow-auto p-1" @scroll="onScroll">
        <li v-for="version in shownVersions" :key="version.commit.sha">
          <button
            type="button"
            class="grid w-full gap-0.5 rounded px-2 py-1.5 text-left text-ui"
            :class="version === picked ? 'bg-elevated' : version === compared ? 'bg-elevated/60 ring-1 ring-accented' : 'hover:bg-elevated/60'"
            @click="pick(version, $event)"
          >
            <span class="flex items-center gap-1.5">
              <span class="min-w-0 flex-1 truncate" :class="version === picked ? 'font-medium text-highlighted' : ''">{{ version.commit.subject }}</span>
              <UBadge v-if="badge(version.status)" :label="badge(version.status)" size="sm" color="neutral" variant="subtle" />
            </span>
            <span class="flex gap-2 text-label text-muted">
              <span class="truncate">{{ version.commit.author }}</span>
              <span class="ml-auto shrink-0 tabular-nums">{{ when(version.commit.time) }}</span>
              <span class="shrink-0 font-mono">{{ version.commit.sha.slice(0, 7) }}</span>
            </span>
            <span v-if="version.status === 'renamed'" class="truncate text-label text-dimmed">{{ t('pages.history.from', { path: version.oldPath ?? '' }) }}</span>
          </button>
        </li>
        <li v-if="loading" class="flex justify-center py-2"><UIcon name="i-lucide-loader-circle" class="size-4 animate-spin text-muted" /></li>
        <li v-else-if="!versions.length && !problem" class="p-3 text-ui text-muted">{{ t('pages.history.noCommits') }}</li>
      </ul>
    </aside>
    <section v-if="picked" class="flex min-h-0 flex-col">
      <div class="flex items-center gap-2 border-b border-default px-3 py-1.5 text-xs text-muted">
        <span class="min-w-0 flex-1 truncate">{{ compared && compared !== picked ? t('pages.history.between', { from: span!.from?.slice(0, 7) ?? '', to: span!.to.slice(0, 7) }) : picked.commit.subject }}</span>
        <UButton v-if="compared && compared !== picked" :label="t('pages.history.stopComparing')" size="xs" color="neutral" variant="ghost" @click="pick(picked)" />
        <UButton :label="t('pages.history.blameAt')" size="xs" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path: picked.path, at: picked.commit.sha } }" />
        <UButton :label="t('pages.history.showCommit')" size="xs" color="neutral" variant="ghost" :to="{ path: '/commits', query: { commit: picked.commit.sha } }" />
      </div>
      <DiffView v-if="span" :key="`${span.from}:${span.to}`" :from="span.from" :to="span.to" :path="span.path" :old-path="span.oldPath" class="min-h-0 flex-1" />
    </section>
  </div>
</template>
