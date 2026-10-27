<script setup lang="ts">
import type { GrepLine, GrepMode, GrepResult } from '~/bindings'

/*
 * Searching the files of a commit (src-tauri/src/git/grep.rs, `git grep` on its tree): HEAD by default, or any branch,
 * tag or commit (`?at=` from the commit menu). The pattern is plain text, a regular expression or a whole word, found
 * in the opened folder's files, optionally only under a path or glob. The matches come by file, with the match marked;
 * a line opens the file as it was at that commit around it, from where its blame is one click away. A search runs on
 * Enter or the button only, never while typing.
 */
const route = useRoute()
const { repo } = useRepositories()
const { refs } = useRefs()
const failure = useFailure()

const at = ref(typeof route.query.at === 'string' && route.query.at ? route.query.at : 'HEAD')
const pattern = ref('')
const mode = ref<GrepMode>('text')
const ignoreCase = ref(false)
const path = ref('')
watch(() => route.query.at, (now) => {
  if (typeof now === 'string' && now) at.value = now
})

const MODES: GrepMode[] = ['text', 'regex', 'word']
/** The branches and tags to search, picked into the commit field. */
const TAGS_LISTED = 30
const revisions = computed(() => [
  [{ label: 'HEAD', icon: 'i-lucide-git-commit-horizontal', onSelect: () => { at.value = 'HEAD' } }],
  (refs.value?.branches ?? []).map(branch => ({ label: branch.name, icon: 'i-lucide-git-branch', onSelect: () => { at.value = branch.name } })),
  [...refs.value?.tags ?? []].sort((a, b) => b.time - a.time).slice(0, TAGS_LISTED).map(tag => ({ label: tag.name, icon: 'i-lucide-tag', onSelect: () => { at.value = tag.name } }))
].filter(group => group.length))

/** The search the results are for: the marking follows it, not the fields as they are now. */
type Asked = { at: string, pattern: string, mode: GrepMode, ignoreCase: boolean }
const result = shallowRef<GrepResult | null>(null)
const asked = shallowRef<Asked | null>(null)
// A slow answer for the tab or the search before lands nowhere.
const ask = latest(repo)
const { loading } = ask
async function search(): Promise<void> {
  if (!repo.value || !pattern.value) return
  const wanted: Asked = { at: at.value.trim() || 'HEAD', pattern: pattern.value, mode: mode.value, ignoreCase: ignoreCase.value }
  const under = path.value.trim() || null
  try {
    const found = await ask.run(where => haunt.grep(where, wanted.at, wanted.pattern, wanted.mode, wanted.ignoreCase, under))
    if (found !== undefined) {
      result.value = found
      asked.value = wanted
    }
  } catch (error) {
    failure.failed(t('pages.grep.searchFailed'), error)
  }
}
watch(repo, () => {
  ask.cancel()
  result.value = null
  asked.value = null
  preview.value = null
})

const lineCount = computed(() => (result.value?.files ?? []).reduce((sum, file) => sum + file.matches.length, 0))

/** What marks the matches in a line, as JavaScript reads the pattern; null when it reads it differently or not at all. */
const marker = computed(() => {
  const search = asked.value
  if (!search) return null
  const plain = search.pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const source = search.mode === 'regex' ? search.pattern : search.mode === 'word' ? `(?<![\\w])${plain}(?![\\w])` : plain
  try {
    return new RegExp(source, search.ignoreCase ? 'gi' : 'g')
  } catch {
    return null
  }
})
type Part = { text: string, hit: boolean }
function parts(text: string): Part[] {
  const found = marker.value
  if (!found) return [{ text, hit: false }]
  const out: Part[] = []
  let from = 0
  found.lastIndex = 0
  for (let match = found.exec(text); match; match = found.exec(text)) {
    if (!match[0]) {
      found.lastIndex++
      continue
    }
    if (match.index > from) out.push({ text: text.slice(from, match.index), hit: false })
    out.push({ text: match[0], hit: true })
    from = match.index + match[0].length
  }
  if (from < text.length) out.push({ text: text.slice(from), hit: false })
  return out
}

/** The file as it was at the searched commit, around the clicked line. */
const AROUND = 10
type Preview = { path: string, line: number, lines: string[] | null, from: number }
const preview = ref<Preview | null>(null)
const previewOpen = computed({
  get: () => !!preview.value,
  set: (open: boolean) => {
    if (!open) preview.value = null
  }
})
const reading = latest(repo)
async function show(file: string, hit: GrepLine): Promise<void> {
  const sha = result.value?.at
  if (!sha) return
  preview.value = { path: file, line: hit.line, lines: null, from: 1 }
  try {
    const from = Math.max(1, hit.line - AROUND)
    const lines = await reading.run(where => haunt.linesAt(where, sha, file, from, hit.line + AROUND))
    if (lines === undefined || preview.value?.path !== file || preview.value.line !== hit.line) return
    preview.value = { ...preview.value, from, lines }
  } catch (error) {
    preview.value = null
    failure.failed(t('pages.grep.readFailed', { path: file }), error)
  }
}
</script>

<template>
  <div class="mx-auto grid w-full max-w-5xl content-start gap-5 px-4 py-8">
    <div class="grid gap-1">
      <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.grep.title') }}</h1>
      <p class="text-sm text-muted">{{ t('pages.grep.intro') }}</p>
    </div>

    <form class="grid gap-3" @submit.prevent="search">
      <div class="flex flex-wrap items-center gap-2">
        <UFieldGroup>
          <UInput v-model="at" icon="i-lucide-git-commit-horizontal" class="w-48" :ui="{ base: 'font-mono' }" placeholder="HEAD" :aria-label="t('pages.grep.at')" :title="t('pages.grep.atHelp')" />
          <UDropdownMenu :items="revisions" :content="{ align: 'start' }" :ui="{ content: 'max-h-80' }">
            <UButton icon="i-lucide-chevron-down" color="neutral" variant="outline" :aria-label="t('pages.grep.pickAt')" />
          </UDropdownMenu>
        </UFieldGroup>
        <UInput v-model="pattern" icon="i-lucide-text-search" class="min-w-56 flex-1" :ui="{ base: 'font-mono' }" :placeholder="t('pages.grep.pattern')" :aria-label="t('pages.grep.pattern')" autofocus />
        <UButton type="submit" :label="t('pages.grep.search')" icon="i-lucide-search" :loading="loading" :disabled="!pattern" />
      </div>
      <div class="flex flex-wrap items-center gap-3">
        <UFieldGroup size="sm">
          <UButton v-for="each in MODES" :key="each" :label="t(`pages.grep.mode.${each}`)" :color="mode === each ? 'primary' : 'neutral'" :variant="mode === each ? 'solid' : 'outline'" @click="mode = each" />
        </UFieldGroup>
        <USwitch v-model="ignoreCase" :label="t('pages.grep.ignoreCase')" />
        <UInput v-model="path" icon="i-lucide-folder" size="sm" class="w-56" :ui="{ base: 'font-mono' }" :placeholder="t('pages.grep.path')" :aria-label="t('pages.grep.path')" :title="t('pages.grep.pathHelp')" />
      </div>
    </form>

    <p v-if="loading && !result" class="text-sm text-muted">{{ t('pages.grep.searching') }}</p>
    <template v-if="result && asked">
      <p v-if="!result.files.length" class="text-sm text-muted">{{ t('pages.grep.nothing', { pattern: asked.pattern, at: asked.at }) }}</p>
      <template v-else>
        <p class="text-xs text-muted">
          {{ t('pages.grep.lines', { count: lineCount }) }} {{ t('pages.grep.files', { count: result.files.length }) }}
          <span class="font-mono">{{ asked.at }} · {{ result.at.slice(0, 7) }}</span>
        </p>
        <p v-if="result.cut" class="text-xs text-warning">{{ t('pages.grep.cut') }}</p>
        <section v-for="file in result.files" :key="file.path" class="grid gap-0.5">
          <div class="flex min-w-0 items-center gap-2 border-b border-default pb-1">
            <span class="truncate font-mono text-sm text-highlighted" :title="file.path">{{ file.path }}</span>
            <UBadge :label="String(file.matches.length)" color="neutral" variant="subtle" size="sm" :title="t('pages.grep.lines', { count: file.matches.length })" />
          </div>
          <button
            v-for="hit in file.matches"
            :key="hit.line"
            type="button"
            class="grid grid-cols-[3.5rem_minmax(0,1fr)] gap-2 rounded px-1 py-0.5 text-left font-mono text-xs hover:bg-elevated/60"
            :title="t('pages.grep.open', { line: hit.line })"
            @click="show(file.path, hit)"
          >
            <span class="text-right text-muted tabular-nums">{{ hit.line }}</span>
            <span class="truncate whitespace-pre"><template v-for="(part, index) in parts(hit.text)" :key="index"><mark v-if="part.hit" class="rounded-sm bg-warning/25 text-highlighted">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></span>
          </button>
        </section>
      </template>
    </template>

    <USlideover v-model:open="previewOpen" :title="preview?.path ?? ''" :description="result ? t('pages.grep.previewAt', { line: preview?.line ?? 0, at: result.at.slice(0, 7) }) : ''" :ui="{ content: 'max-w-3xl', title: 'font-mono' }">
      <template #body>
        <div v-if="preview" class="grid gap-3">
          <div class="flex flex-wrap gap-2">
            <UButton :label="t('pages.grep.blame')" icon="i-lucide-user-round-search" size="xs" color="neutral" variant="outline" :to="{ path: '/blame', query: { path: preview.path, at: result?.at } }" />
            <UButton :label="t('pages.grep.showCommit')" icon="i-lucide-eye" size="xs" color="neutral" variant="ghost" :to="{ path: '/commits', query: { commit: result?.at } }" />
          </div>
          <p v-if="!preview.lines" class="text-sm text-muted">{{ t('pages.grep.reading') }}</p>
          <ol v-else class="overflow-x-auto rounded border border-default bg-elevated/30 py-1 font-mono text-xs">
            <li v-for="(line, index) in preview.lines" :key="index" class="grid grid-cols-[3.5rem_minmax(0,1fr)] gap-2 px-1" :class="preview.from + index === preview.line ? 'bg-warning/15 text-highlighted' : ''">
              <span class="text-right text-muted tabular-nums select-none">{{ preview.from + index }}</span>
              <span class="whitespace-pre"><template v-if="preview.from + index === preview.line"><template v-for="(part, spot) in parts(line)" :key="spot"><mark v-if="part.hit" class="rounded-sm bg-warning/30 text-highlighted">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></template><template v-else>{{ line }}</template></span>
            </li>
          </ol>
        </div>
      </template>
    </USlideover>
  </div>
</template>
