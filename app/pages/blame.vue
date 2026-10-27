<script setup lang="ts">
import type { CommitInfo, FileBlame } from '~/bindings'

/*
 * Blame for a whole file: each line with the commit that last changed it, as the file is now (lines not committed yet
 * say so) or at a commit. A run of lines from one commit shows the commit once, at its first line; the stripe beside
 * it is darker the newer the commit. Click a commit to see it; the arrow beside it blames the file as it was before
 * that commit, to step back past a line's last change.
 */
const { repo } = useRepositories()
const route = useRoute()
const { times } = usePreferences()

const path = computed(() => (typeof route.query.path === 'string' ? route.query.path : ''))
const at = computed(() => (typeof route.query.at === 'string' ? route.query.at : null))
const blame = shallowRef<FileBlame | null>(null)
const problem = ref('')
const ask = latest(repo)

watch([path, at, repo], async () => {
  blame.value = null
  problem.value = ''
  if (!path.value) return
  try {
    const found = await ask.run(where => haunt.fileBlame(where, path.value, at.value))
    if (found !== undefined) blame.value = found
  } catch (error) {
    problem.value = errorMessage(error, t('pages.blame.readError'))
  }
}, { immediate: true })

const NOT_YET = /^0+$/
const commits = computed(() => new Map((blame.value?.commits ?? []).map(commit => [commit.sha, commit] as [string, CommitInfo])))
/** 0 for the oldest commit of the file, 1 for the newest: the stripe's strength. */
const age = computed(() => {
  const list = (blame.value?.commits ?? []).filter(commit => !NOT_YET.test(commit.sha))
  const oldest = Math.min(...list.map(commit => commit.time))
  const newest = Math.max(...list.map(commit => commit.time))
  return (commit: CommitInfo | undefined) => (!commit || NOT_YET.test(commit.sha) ? 1 : newest > oldest ? (commit.time - oldest) / (newest - oldest) : 1)
})
const when = (seconds: number) => times.value === 'date'
  ? new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
  : new Date(seconds * 1000).toLocaleDateString()
</script>

<template>
  <div v-if="!path" class="py-16 text-center text-muted">{{ t('pages.blame.pickFile') }}</div>
  <div v-else class="flex h-full flex-col">
    <div class="flex items-center gap-2 border-b border-default px-3 py-2">
      <UIcon name="i-lucide-user-round-search" class="size-4 shrink-0 text-muted" />
      <span class="min-w-0 truncate font-mono text-ui text-highlighted" :title="path">{{ path }}</span>
      <span class="text-ui text-muted">{{ at ? at.endsWith('^') ? t('pages.blame.atParent', { sha: at.slice(0, 7) }) : t('pages.blame.at', { sha: at.slice(0, 7) }) : t('pages.blame.now') }}</span>
      <div class="ml-auto flex gap-1">
        <UButton v-if="at" :label="t('pages.blame.asNow')" size="xs" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path } }" />
        <UButton :label="t('pages.blame.history')" icon="i-lucide-history" size="xs" color="neutral" variant="ghost" :to="{ path: '/history', query: { path } }" />
      </div>
    </div>
    <div v-if="problem && /no such path/.test(problem)" class="grid justify-items-start gap-2 p-4 text-ui text-muted">
      <p>{{ at?.endsWith('^') ? t('pages.blame.notThereBefore') : t('pages.blame.notThereAt') }}</p>
      <UButton :label="t('pages.blame.trace')" icon="i-lucide-history" size="xs" color="neutral" variant="outline" :to="{ path: '/history', query: { path } }" />
    </div>
    <p v-else-if="problem" class="p-3 text-ui text-error">{{ problem }}</p>
    <div v-else-if="!blame" class="grid justify-items-center py-10"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
    <div v-else class="min-h-0 flex-1 overflow-auto">
      <table class="w-full border-collapse font-mono text-xs">
        <tbody>
          <tr v-for="(line, index) in blame.lines" :key="index" :class="index > 0 && blame.lines[index - 1]!.sha !== line.sha ? 'border-t border-default/60' : ''">
            <td class="w-72 max-w-72 truncate px-2 align-top font-sans text-label text-muted select-none">
              <template v-if="index === 0 || blame.lines[index - 1]!.sha !== line.sha">
                <span v-if="NOT_YET.test(line.sha)" class="text-warning">{{ t('pages.blame.notCommitted') }}</span>
                <span v-else class="flex items-center gap-1">
                  <NuxtLink
                    :to="{ path: '/blame', query: { path, at: `${line.sha}^` } }"
                    class="grid size-4 shrink-0 place-items-center rounded hover:bg-elevated hover:text-highlighted"
                    :title="t('pages.blame.beforeHelp')"
                    :aria-label="t('pages.blame.before')"
                  ><UIcon name="i-lucide-undo-2" class="size-3" /></NuxtLink>
                  <NuxtLink :to="{ path: '/commits', query: { commit: line.sha } }" class="min-w-0 truncate hover:underline" :title="commits.get(line.sha)?.subject">
                    <span class="font-mono">{{ line.sha.slice(0, 7) }}</span>
                    {{ commits.get(line.sha)?.author }} · {{ when(commits.get(line.sha)?.time ?? 0) }}
                  </NuxtLink>
                </span>
              </template>
            </td>
            <td class="w-1 p-0" :style="{ background: `color-mix(in oklab, var(--ui-primary) ${Math.round(15 + age(commits.get(line.sha)) * 70)}%, transparent)` }" />
            <td class="w-12 px-2 text-right align-top text-dimmed tabular-nums select-none">{{ index + 1 }}</td>
            <td class="px-2 whitespace-pre">{{ line.text || ' ' }}</td>
          </tr>
        </tbody>
      </table>
      <p v-if="blame.cut" class="p-3 text-ui text-muted">{{ t('pages.blame.cut', { count: blame.lines.length }) }}</p>
    </div>
  </div>
</template>
