<script setup lang="ts">
import type { ChangedFile, Incoming } from '~/bindings'

/*
 * What a pull would bring in, before pulling: the upstream's commits since your branch and it went apart, and what they
 * change, function by function. A function you changed too (in your commits or in your files) is marked: that is
 * where a pull may conflict, seen before it happens.
 */
const { repo, gitChanged } = useRepositories()
const network = useNetwork()
const { times } = usePreferences()

const incoming = shallowRef<Incoming | null>(null)
const theirs = shallowRef<ChangedFile[]>([])
const mine = shallowRef<Set<string>>(new Set())
const problem = ref('')
const ask = latest(repo)
const { loading } = ask
const key = (path: string, qualified: string) => `${path}#${qualified}`

async function load(): Promise<void> {
  problem.value = ''
  try {
    const found = await ask.run(async (at) => {
      const upcoming = await haunt.incoming(at)
      if (!upcoming || !upcoming.commits.length) return { upcoming, files: [] as ChangedFile[], local: new Set<string>() }
      const [range, local] = await Promise.all([
        haunt.range(at, upcoming.base, upcoming.tip),
        haunt.changes(at, { kind: 'ref', ref: upcoming.base }).catch(() => null)
      ])
      const changedHere = new Set((local?.files ?? []).flatMap(file => file.functions.map(fn => key(file.path, fn.qualified))))
      return { upcoming, files: range.files.filter(file => file.code), local: changedHere }
    })
    if (found === undefined) return
    incoming.value = found.upcoming
    theirs.value = found.files
    mine.value = found.local
  } catch (error) {
    problem.value = errorMessage(error, t('pages.incoming.readError'))
  }
}
watch([repo, gitChanged('refs', 'head')], () => repo.value && void load(), { immediate: true })

const both = computed(() => theirs.value.flatMap(file => file.functions.filter(fn => mine.value.has(key(file.path, fn.qualified))).map(fn => ({ file, fn }))))
const functions = computed(() => theirs.value.reduce((sum, file) => sum + file.functions.length, 0))
const when = (seconds: number) => times.value === 'date'
  ? new Date(seconds * 1000).toLocaleString(locale(), { dateStyle: 'short', timeStyle: 'short' })
  : new Date(seconds * 1000).toLocaleDateString(locale())
const STATUS: Record<string, string> = { added: 'text-success', removed: 'text-error', changed: 'text-warning', modified: 'text-warning', renamed: 'text-info', moved: 'text-info' }
</script>

<template>
  <div class="mx-auto grid max-w-5xl content-start gap-4 p-5">
    <div class="flex items-start gap-3">
      <div class="min-w-0 flex-1">
        <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.incoming.title') }}</h1>
        <p v-if="incoming" class="text-ui text-muted">
          {{ incoming.ahead
            ? t('pages.incoming.summaryAhead', { upstream: incoming.upstream, count: incoming.commits.length, ahead: incoming.ahead })
            : t('pages.incoming.summary', { upstream: incoming.upstream, count: incoming.commits.length }) }}
          <template v-if="incoming.commits.length">{{ t('pages.incoming.theyChange', { count: functions, files: t('pages.incoming.files', { count: theirs.length }) }) }}</template>
        </p>
        <p v-else-if="!loading && !problem" class="text-ui text-muted">{{ t('pages.incoming.noUpstream') }}</p>
      </div>
      <UButton v-if="incoming?.commits.length" :label="t('pages.incoming.pull')" icon="i-lucide-arrow-down-to-line" :disabled="!network.trusted.value || !!network.running.value" @click="network.pull()" />
    </div>
    <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <div v-if="loading && !incoming" class="grid justify-items-center py-6"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>

    <template v-if="incoming?.commits.length">
      <UAlert
        v-if="both.length"
        color="warning"
        variant="subtle"
        icon="i-lucide-git-merge"
        :title="t('pages.incoming.both', { count: both.length })"
        :description="t('pages.incoming.bothHelp')"
      />
      <UAlert v-else color="success" variant="subtle" icon="i-lucide-check" :title="t('pages.incoming.noneBoth')" :description="t('pages.incoming.noneBothHelp')" />

      <section v-if="both.length" class="grid gap-1">
        <h2 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('pages.incoming.changedBoth') }}</h2>
        <ul class="grid gap-1">
          <li v-for="item in both" :key="key(item.file.path, item.fn.qualified)" class="flex items-center gap-2 rounded-md border border-warning/40 px-3 py-1.5 text-ui">
            <span class="font-mono text-highlighted">{{ item.fn.qualified }}</span>
            <span class="min-w-0 truncate font-mono text-xs text-muted">{{ item.file.path }}</span>
          </li>
        </ul>
      </section>

      <section class="grid gap-1">
        <h2 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('pages.incoming.whatTheyChange') }}</h2>
        <div v-for="file in theirs" :key="file.path" class="rounded-md border border-default">
          <p class="flex items-center gap-2 border-b border-default px-3 py-1.5 font-mono text-xs">
            <span class="min-w-0 flex-1 truncate text-highlighted">{{ file.path }}</span>
            <span class="text-success">+{{ file.added }}</span><span class="text-error">−{{ file.removed }}</span>
          </p>
          <ul v-if="file.functions.length" class="grid p-1">
            <li v-for="fn in file.functions" :key="fn.qualified" class="flex items-center gap-2 rounded px-2 py-0.5 text-xs" :class="mine.has(key(file.path, fn.qualified)) ? 'bg-warning/10' : ''">
              <span class="w-16 shrink-0" :class="STATUS[fn.status] ?? 'text-muted'">{{ t(`pages.incoming.status.${fn.status}`) }}</span>
              <span class="min-w-0 flex-1 truncate font-mono">{{ fn.qualified }}</span>
              <UBadge v-if="mine.has(key(file.path, fn.qualified))" :label="t('pages.incoming.youToo')" color="warning" variant="subtle" size="sm" />
            </li>
          </ul>
          <p v-else class="px-3 py-1.5 text-xs text-muted">{{ t('pages.incoming.noFunctions') }}</p>
        </div>
      </section>

      <section class="grid gap-1">
        <h2 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('pages.incoming.commits') }}</h2>
        <ul class="grid gap-0.5">
          <li v-for="commit in incoming.commits" :key="commit.sha">
            <NuxtLink :to="{ path: '/commits', query: { commit: commit.sha } }" class="flex items-center gap-3 rounded px-2 py-1 text-ui hover:bg-elevated">
              <span class="font-mono text-xs text-muted">{{ commit.sha.slice(0, 7) }}</span>
              <span class="min-w-0 flex-1 truncate">{{ commit.subject }}</span>
              <span class="text-xs text-muted">{{ commit.author }}</span>
              <span class="text-xs text-muted tabular-nums">{{ when(commit.time) }}</span>
            </NuxtLink>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
