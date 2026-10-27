<script setup lang="ts">
import type { Blamed, FileAuthor } from '~/bindings'

/*
 * Who knows a part, or the folder of it shown: who wrote the lines it has now (git blame at HEAD, up to 300 files), who
 * changed it in the last year, and a warning where one person wrote nearly all of it and nobody else changed it
 * lately, in a repository others work on too: the code only they know.
 */
const props = defineProps<{ paths: string[] }>()
const { repo, gitChanged } = useRepositories()
const { times } = usePreferences()
const headChanged = gitChanged('head')

const blamed = shallowRef<Blamed | null>(null)
const recent = shallowRef<FileAuthor[] | null>(null)
const askBlame = latest(repo)
const askRecent = latest(repo)
const { loading } = askBlame

watch([() => props.paths.join('\n'), repo, headChanged], async () => {
  blamed.value = null
  if (!props.paths.length) return askBlame.cancel()
  const found = await askBlame.run(at => haunt.blameAuthors(at, props.paths)).catch(() => null)
  if (found !== undefined) blamed.value = found
}, { immediate: true })
watch([repo, headChanged], async () => {
  const found = await askRecent.run(at => haunt.recentAuthors(at, 365)).catch(() => [])
  if (found !== undefined) recent.value = found
}, { immediate: true })

const total = computed(() => blamed.value?.authors.reduce((sum, author) => sum + author.lines, 0) ?? 0)
/** Who changed these files in the last year, most changed lines first. */
const lately = computed(() => {
  const wanted = new Set(props.paths)
  const by = new Map<string, { name: string, commits: number, lines: number, last: number }>()
  for (const item of recent.value ?? []) {
    if (!wanted.has(item.path)) continue
    const known = by.get(item.author) ?? { name: item.author, commits: 0, lines: 0, last: 0 }
    known.commits += item.commits
    known.lines += item.lines
    known.last = Math.max(known.last, item.last)
    by.set(item.author, known)
  }
  return [...by.values()].sort((a, b) => b.lines - a.lines)
})
/** One person wrote 80% or more of it, and nobody else changed it in the last year, in a repository others work on
 * too (in a project of one person, everything is theirs: no warning). */
const onlyOne = computed(() => {
  const top = blamed.value?.authors[0]
  if (new Set((recent.value ?? []).map(item => item.author)).size < 2) return null
  if (!top || total.value < 50 || top.lines / total.value < 0.8) return null
  return lately.value.every(item => item.name === top.name) ? top.name : null
})
const when = (seconds: number) => times.value === 'date'
  ? new Date(seconds * 1000).toLocaleDateString()
  : ago(seconds)
function ago(seconds: number): string {
  const days = Math.floor((Date.now() / 1000 - seconds) / 86400)
  if (days < 1) return t('PartKnowledge.today')
  if (days < 2) return t('PartKnowledge.yesterday')
  if (days < 60) return t('PartKnowledge.daysAgo', { count: days })
  return t('PartKnowledge.monthsAgo', { count: Math.round(days / 30) })
}
</script>

<template>
  <section v-if="paths.length" class="grid gap-2 rounded-md border border-default bg-elevated/30 px-3 py-2">
    <h3 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
      {{ t('PartKnowledge.title') }}
      <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-3 animate-spin" />
    </h3>
    <p v-if="onlyOne" class="flex items-start gap-1.5 text-xs text-warning">
      <UIcon name="i-lucide-triangle-alert" class="mt-0.5 size-3.5 shrink-0" />
      <span><b>{{ onlyOne }}</b> {{ t('PartKnowledge.onlyOne') }}</span>
    </p>
    <div class="grid gap-x-6 gap-y-2 sm:grid-cols-2">
      <div class="grid content-start gap-1">
        <p class="text-label text-muted">{{ t('PartKnowledge.wrote') }}</p>
        <template v-if="blamed && total">
          <div class="flex h-1.5 overflow-hidden rounded-full bg-elevated">
            <i v-for="(author, n) in blamed.authors.slice(0, 6)" :key="author.name" class="block h-full" :style="{ width: `${author.lines / total * 100}%`, background: `color-mix(in srgb, var(--c, var(--ui-primary)) ${100 - n * 16}%, var(--ui-bg-elevated))` }" :title="`${author.name}: ${t('PartKnowledge.lines', { count: author.lines })}`" />
          </div>
          <p class="text-xs">
            <template v-for="(author, n) in blamed.authors.slice(0, 4)" :key="author.name">{{ n ? ' · ' : '' }}{{ author.name }} <span class="text-muted">{{ Math.round(author.lines / total * 100) }}%</span></template>
          </p>
          <p v-if="blamed.skipped" class="text-label text-dimmed">{{ t('PartKnowledge.skipped', { count: blamed.skipped }) }}</p>
        </template>
        <p v-else-if="blamed" class="text-xs text-muted">{{ t('PartKnowledge.noLines') }}</p>
      </div>
      <div class="grid content-start gap-1">
        <p class="text-label text-muted">{{ t('PartKnowledge.changed') }}</p>
        <p v-if="recent && !lately.length" class="text-xs text-muted">{{ t('PartKnowledge.nobody') }}</p>
        <ul v-else class="grid gap-0.5">
          <li v-for="item in lately.slice(0, 4)" :key="item.name" class="flex gap-2 text-xs">
            <span class="min-w-0 flex-1 truncate">{{ item.name }}</span>
            <span class="text-muted tabular-nums">{{ t('PartKnowledge.commits', { count: item.commits }) }} · {{ when(item.last) }}</span>
          </li>
        </ul>
      </div>
    </div>
  </section>
</template>
