<script setup lang="ts">
import type { ChangedFile, LineSpanAt } from '~/bindings'

/*
 * Who to ask for a review: the people who wrote the lines a change touches, as they were before it (git blame at the
 * base), most lines first, leaving out you and the change's own authors. Added functions have no earlier lines, so
 * they do not count; a moved one counts where it was.
 */
const props = defineProps<{ base: string | null, files: ChangedFile[], exclude?: string[] }>()
const { repo } = useRepositories()

const spans = computed<LineSpanAt[]>(() => props.files.flatMap(file => file.functions.flatMap((fn) => {
  if (fn.from) return [{ path: fn.from.path, line: fn.from.line, endLine: fn.from.endLine }]
  return fn.before ? [{ path: file.oldPath, line: fn.before.line, endLine: fn.before.endLine }] : []
})))
const found = shallowRef<{ name: string, lines: number }[] | null>(null)
const ask = latest(repo)
watch([() => props.base, () => JSON.stringify(spans.value), repo], async () => {
  found.value = null
  if (!props.base || !spans.value.length) return
  const answer = await ask.run(at => haunt.reviewers(at, props.base!, spans.value)).catch(() => null)
  if (answer === undefined) return
  const left = new Set([...(props.exclude ?? []), ...(answer?.you ? [answer.you] : [])])
  found.value = (answer?.authors ?? []).filter(author => !left.has(author.name))
}, { immediate: true })
const total = computed(() => found.value?.reduce((sum, author) => sum + author.lines, 0) ?? 0)
</script>

<template>
  <section class="grid gap-1.5">
    <h2 class="font-semibold text-highlighted">{{ t('SuggestedReviewers.title') }}</h2>
    <p v-if="!base || !spans.length" class="text-xs text-muted">{{ t('SuggestedReviewers.onlyAdds') }}</p>
    <p v-else-if="!found" class="text-xs text-muted">{{ t('SuggestedReviewers.reading') }}</p>
    <p v-else-if="!found.length" class="text-xs text-muted">{{ t('SuggestedReviewers.onlyYou') }}</p>
    <template v-else>
      <p class="text-xs text-muted">{{ t('SuggestedReviewers.intro') }}</p>
      <ul class="grid gap-0.5">
        <li v-for="author in found.slice(0, 3)" :key="author.name" class="flex items-center gap-2 rounded px-2 py-0.5 text-xs">
          <UIcon name="i-lucide-user-round" class="size-3.5 shrink-0 text-muted" />
          <span class="min-w-0 flex-1 truncate text-highlighted">{{ author.name }}</span>
          <span class="text-muted tabular-nums">{{ t('SuggestedReviewers.lines', { count: author.lines }) }} · {{ Math.round(author.lines / total * 100) }}%</span>
        </li>
      </ul>
    </template>
  </section>
</template>
