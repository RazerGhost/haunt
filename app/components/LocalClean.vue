<script setup lang="ts">
/*
 * Local changes with nothing changed: the working tree is clean, on which branch; how the branch stands against its
 * upstream, with Push and Pull when it is ahead or behind (or Push to set one when it has none); the last commit, with
 * Amend (the commit box opens to reword it); and the stashes, to apply or pop.
 */
const emit = defineEmits<{ amend: [] }>()
const { refs, current } = useRefs()
const { commitBox } = useLocalChanges()
const network = useNetwork()
const actions = useRefActions()
const { times } = usePreferences()

const branch = computed(() => current.value?.name ?? null)
const lastSubject = computed(() => (commitBox.value?.lastMessage ?? '').split('\n')[0] || current.value?.subject || '')
const lastSha = computed(() => current.value?.sha ?? refs.value?.sha ?? null)
const stashes = computed(() => refs.value?.stashes ?? [])
const hasRemote = computed(() => !!refs.value?.remotes.length)
// Pull and push run only in a trusted folder; the buttons say so.
const networkTitle = computed(() => (network.trusted.value ? undefined : t('LocalClean.untrusted')))
const networkOff = computed(() => !network.trusted.value || !!network.running.value)

const ago = (seconds: number) => {
  if (times.value === 'date') return new Date(seconds * 1000).toLocaleString(locale(), { dateStyle: 'short', timeStyle: 'short' })
  const minutes = Math.floor((Date.now() / 1000 - seconds) / 60)
  if (minutes < 60) return t('LocalClean.minutes', { count: Math.max(1, minutes) })
  if (minutes < 60 * 24) return t('LocalClean.hours', { count: Math.floor(minutes / 60) })
  if (minutes < 60 * 24 * 60) return t('LocalClean.days', { count: Math.floor(minutes / 60 / 24) })
  return new Date(seconds * 1000).toLocaleDateString(locale())
}
const day = (seconds: number) => new Date(seconds * 1000).toLocaleString(locale())
</script>

<template>
  <div class="grid min-h-0 flex-1 content-start gap-4 overflow-y-auto p-6 text-sm">
    <header class="flex items-center gap-2">
      <UIcon name="i-lucide-circle-check" class="size-5 shrink-0 text-success" />
      <h2 class="font-semibold text-highlighted">{{ t('LocalClean.clean') }}</h2>
      <UBadge v-if="branch" :label="branch" icon="i-lucide-git-branch" size="sm" variant="subtle" color="neutral" class="font-mono" />
      <UBadge v-else-if="lastSha" :label="t('LocalClean.detached', { sha: lastSha.slice(0, 7) })" size="sm" variant="subtle" color="warning" class="font-mono" />
    </header>

    <section v-if="current" class="grid gap-2 rounded-md border border-default p-3">
      <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('LocalClean.upstream') }}</h3>
      <div class="flex flex-wrap items-center gap-2">
        <template v-if="current.upstream">
          <span class="font-mono text-xs text-highlighted">{{ current.upstream }}</span>
          <span v-if="current.gone" class="text-warning">{{ t('LocalClean.gone') }}</span>
          <span v-else-if="!current.ahead && !current.behind" class="text-muted">{{ t('LocalClean.upToDate') }}</span>
          <span v-else class="text-muted">
            <template v-if="current.ahead">{{ t('LocalClean.ahead', { count: current.ahead }) }}</template>
            <template v-if="current.ahead && current.behind"> · </template>
            <template v-if="current.behind">{{ t('LocalClean.behind', { count: current.behind }) }}</template>
          </span>
          <span class="flex-1" />
          <UButton icon="i-lucide-refresh-cw" :label="t('LocalClean.fetch')" size="xs" color="neutral" variant="ghost" :disabled="!!network.running.value" :loading="network.running.value?.op === 'fetch'" @click="network.fetch()" />
          <UButton v-if="current.behind && !current.gone" icon="i-lucide-arrow-down" :label="t('LocalClean.pull', { count: current.behind })" size="xs" color="neutral" variant="outline" :title="networkTitle" :disabled="networkOff" :loading="network.running.value?.op === 'pull'" @click="network.pull()" />
          <UButton v-if="current.ahead || current.gone" icon="i-lucide-arrow-up" :label="current.gone ? t('LocalClean.push') : t('LocalClean.pushCount', { count: current.ahead })" size="xs" :title="networkTitle" :disabled="networkOff" :loading="network.running.value?.op === 'push'" @click="network.push()" />
        </template>
        <template v-else-if="hasRemote">
          <span class="flex-1 text-muted">{{ t('LocalClean.noUpstream') }}</span>
          <UButton icon="i-lucide-arrow-up" :label="t('LocalClean.pushSet')" size="xs" :title="networkTitle" :disabled="networkOff" :loading="network.running.value?.op === 'push'" @click="network.push()" />
        </template>
        <span v-else class="text-muted">{{ t('LocalClean.noRemote') }}</span>
      </div>
    </section>

    <section v-if="lastSha && lastSubject" class="grid gap-2 rounded-md border border-default p-3">
      <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('LocalClean.lastCommit') }}</h3>
      <div class="flex min-w-0 items-center gap-2">
        <NuxtLink :to="{ path: '/commits', query: { commit: lastSha } }" class="min-w-0 flex-1 truncate text-highlighted hover:underline" :title="lastSubject">{{ lastSubject }}</NuxtLink>
        <span v-if="current" class="shrink-0 text-label text-muted" :title="day(current.time)">{{ ago(current.time) }}</span>
        <UButton v-if="commitBox?.lastMessage" icon="i-lucide-pencil" :label="t('LocalClean.amend')" size="xs" color="neutral" variant="outline" :title="t('LocalClean.amendHelp')" @click="emit('amend')" />
      </div>
    </section>

    <section v-if="stashes.length" class="grid gap-2 rounded-md border border-default p-3">
      <h3 class="text-label font-semibold tracking-wider text-muted uppercase">{{ t('LocalClean.stashes') }} <span class="font-normal tabular-nums">{{ stashes.length }}</span></h3>
      <ul class="grid gap-1">
        <li v-for="item in stashes" :key="item.sha" class="flex min-w-0 items-center gap-2">
          <UIcon name="i-lucide-archive" class="size-3.5 shrink-0 text-muted" />
          <span class="min-w-0 flex-1 truncate" :title="item.message">{{ item.message }}</span>
          <span class="shrink-0 text-label text-muted" :title="day(item.time)">{{ ago(item.time) }}</span>
          <UButton :label="t('LocalClean.apply')" size="xs" color="neutral" variant="ghost" :title="t('LocalClean.applyHelp')" :disabled="actions.busy.value" @click="actions.applyStash(item.index, false)" />
          <UButton :label="t('LocalClean.pop')" size="xs" color="neutral" variant="outline" :title="t('LocalClean.popHelp')" :disabled="actions.busy.value" @click="actions.applyStash(item.index, true)" />
        </li>
      </ul>
    </section>
  </div>
</template>
