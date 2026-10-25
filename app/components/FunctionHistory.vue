<script setup lang="ts">
import type { FunctionHistory } from '~~/shared/types'

// A function's history: who wrote its lines, when they last changed, and the commits that touched them (git log -L),
// each a link to it in the commit list. Where it came from: the commit that added it (when the history reaches back that
// far) and the last one that changed how it works, past the ones that only changed spacing or comments (marked). A
// commit that moved it here from another file says so, and the commits before it are those of the function where it
// was. Every commit that added or removed its name is one click away (the commit list's text in changes).
const props = defineProps<{ path: string, start: number, end: number, name?: string }>()
const { scan } = useCode()
const { repo, gitChanged } = useRepositories()
const headChanged = gitChanged('head')

const history = shallowRef<FunctionHistory | null>(null)
const all = ref(false)
const ask = latest(repo)
const { loading } = ask

async function load(): Promise<void> {
  try {
    const found = await ask.run(at => haunt.history(at, props.path, props.start, props.end))
    if (found === undefined) return
    history.value = found
  } catch {
    history.value = null
  }
}
watch(() => [props.path, props.start, props.end, scan.value?.scannedAt, headChanged.value], () => {
  all.value = false
  void load()
}, { immediate: true })

const total = computed(() => history.value?.authors.reduce((sum, author) => sum + author.lines, 0) ?? 0)
const shown = computed(() => (all.value ? history.value?.commits : history.value?.commits.slice(0, 5)) ?? [])
const cosmetic = computed(() => new Set(history.value?.cosmetic ?? []))
/** The commit that added it: the oldest, when the history reaches back to it. */
const born = computed(() => (history.value?.complete ? history.value.commits.at(-1) ?? null : null))
/** The last commit that changed how it works: the newest one that was not only cosmetic. */
const behaviour = computed(() => history.value?.commits.find(commit => !cosmetic.value.has(commit.sha)) ?? null)
const moveAt = computed(() => new Map((history.value?.moves ?? []).map(move => [move.sha, move])))
function moveText(sha: string): string {
  const move = moveAt.value.get(sha)!
  const renamed = move.fromName !== move.toName
  return `Moved here from ${tailOf(move.fromPath)}${renamed ? `, where it was ${move.fromName}` : ''}. Before this, its history there:`
}

function ago(seconds: number): string {
  const days = Math.floor((Date.now() / 1000 - seconds) / 86400)
  if (days < 1) return 'today'
  if (days < 2) return 'yesterday'
  if (days < 60) return `${days} days ago`
  if (days < 730) return `${Math.round(days / 30)} months ago`
  return `${Math.round(days / 365)} years ago`
}
</script>

<template>
  <section class="grid gap-1.5">
    <h3 class="flex items-center gap-2 text-xs font-semibold tracking-wider text-muted uppercase">
      History
      <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-3 animate-spin" />
    </h3>
    <p v-if="history && !history.commits.length && !history.authors.length" class="text-xs text-muted">No history: the file is not in git.</p>
    <template v-else-if="history">
      <p v-if="history.latest" class="text-xs text-muted">
        Last changed {{ ago(history.latest.time) }} by <b class="text-highlighted">{{ history.latest.author }}</b>
      </p>
      <dl v-if="born || behaviour" class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-0.5 text-xs">
        <template v-if="behaviour">
          <dt class="text-muted">Last changed how it works</dt>
          <dd class="min-w-0"><NuxtLink :to="{ path: '/commits', query: { commit: behaviour.sha } }" class="block truncate hover:underline" :title="`${behaviour.subject} · ${behaviour.author}`">{{ behaviour.subject }}</NuxtLink></dd>
        </template>
        <template v-if="born">
          <dt class="text-muted">Born in</dt>
          <dd class="min-w-0"><NuxtLink :to="{ path: '/commits', query: { commit: born.sha } }" class="block truncate hover:underline" :title="`${born.subject} · ${born.author} · ${ago(born.time)}`">{{ born.subject }} <span class="text-muted">· {{ ago(born.time) }}</span></NuxtLink></dd>
        </template>
      </dl>
      <div v-if="total" class="grid gap-1">
        <div class="flex h-1.5 overflow-hidden rounded-full bg-elevated">
          <i v-for="(author, n) in history.authors" :key="author.name" class="block h-full" :style="{ width: `${author.lines / total * 100}%`, background: `color-mix(in srgb, var(--c) ${100 - n * 22}%, var(--ui-bg-elevated))` }" :title="`${author.name}: ${author.lines} lines`" />
        </div>
        <p class="text-label text-muted">
          <template v-for="(author, n) in history.authors.slice(0, 4)" :key="author.name">{{ n ? ' · ' : '' }}{{ author.name }} {{ Math.round(author.lines / total * 100) }}%</template>
        </p>
      </div>
      <ul v-if="shown.length" class="grid gap-1">
        <li v-for="commit in shown" :key="commit.sha">
          <NuxtLink :to="{ path: '/commits', query: { commit: commit.sha } }" class="grid rounded px-2 py-1 hover:bg-elevated" title="See it in the commit list">
            <span class="truncate text-xs text-highlighted" :title="commit.subject">{{ commit.subject }}</span>
            <span class="truncate font-mono text-label text-muted">{{ commit.sha.slice(0, 7) }} · {{ commit.author }} · {{ ago(commit.time) }}<template v-if="cosmetic.has(commit.sha)"> · spacing or comments only</template></span>
          </NuxtLink>
          <p v-if="moveAt.has(commit.sha)" class="flex items-start gap-1 px-2 py-1 text-label text-info" :title="moveAt.get(commit.sha)!.fromPath">
            <UIcon name="i-lucide-move-right" class="mt-0.5 size-3 shrink-0" />{{ moveText(commit.sha) }}
          </p>
        </li>
      </ul>
      <div class="flex flex-wrap items-center gap-1">
        <UButton v-if="!all && history.commits.length > 5" :label="`All ${history.commits.length} commits`" size="xs" color="neutral" variant="ghost" @click="all = true" />
        <UButton v-if="props.name" :label="`Commits mentioning ${props.name}`" icon="i-lucide-text-search" size="xs" color="neutral" variant="ghost" :to="{ path: '/commits', query: { changed: props.name } }" title="Commits that added or removed this name" />
      </div>
    </template>
  </section>
</template>
