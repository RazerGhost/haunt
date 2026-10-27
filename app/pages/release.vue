<script setup lang="ts">
/*
 * Release notes between two tags (shared/releaseNotes.ts): the commits grouped by the part they changed most, and what
 * each part's functions gained, changed and lost, as Markdown to edit and copy. Worked out here, without AI. Opened
 * from the palette or a tag's menu (`?to=<tag>`); the older tag defaults to the one before.
 */
import { releaseNotes } from '~~/shared/releaseNotes'

const route = useRoute()
const { repo } = useRepositories()
const { refs } = useRefs()
const { index } = useCode()
const failure = useFailure()
const toast = useToast()

/** Newest first; tags made in the same second by their names, v1.10 after v1.9. */
const tags = computed(() => [...refs.value?.tags ?? []].sort((a, b) => b.time - a.time || b.name.localeCompare(a.name, undefined, { numeric: true })))
// The From choice with no older tag: the first commit (labelled in the window's language).
const FIRST = '(the first commit)'
const to = ref(typeof route.query.to === 'string' ? route.query.to : '')
const from = ref('')
// The newest tag, and the one before the chosen one, once the tags are in.
watch([tags, to], ([list]) => {
  if (!list.length) return
  if (!to.value) to.value = list[0]!.name
  if (!from.value) {
    const at = list.findIndex(tag => tag.name === to.value)
    from.value = at >= 0 && list[at + 1] ? list[at + 1]!.name : FIRST
  }
}, { immediate: true })
const toItems = computed(() => ['HEAD', ...tags.value.map(tag => tag.name)])
const fromItems = computed(() => [...tags.value.map(tag => ({ label: tag.name, value: tag.name })), { label: t('pages.release.first'), value: FIRST }])

const text = ref('')
const loading = ref(false)
const count = ref(0)
async function make(): Promise<void> {
  const at = repo.value
  if (!at || !to.value) return
  const older = from.value === FIRST ? null : from.value
  loading.value = true
  try {
    // From the first commit there is no older version to compare functions with: the commits only.
    const [commits, range] = await Promise.all([haunt.releaseCommits(at, older, to.value), older ? haunt.range(at, older, to.value) : Promise.resolve(null)])
    const parts = new Map<string, string>()
    for (const [part, files] of index.value?.filesByPart ?? []) for (const file of files) parts.set(file.path, part)
    count.value = commits.length
    text.value = releaseNotes({
      from: older,
      to: to.value,
      commits: commits.map(item => ({ sha: item.commit.sha, subject: item.commit.subject, paths: item.paths })),
      files: range?.files ?? [],
      partOfPath: path => parts.get(path) ?? null
    })
  } catch (error) {
    failure.failed(t('pages.release.failed'), error, t('pages.release.readFailed'))
  } finally {
    loading.value = false
  }
}
async function copy(): Promise<void> {
  await navigator.clipboard?.writeText(text.value)
  toast.add({ title: t('pages.release.copied'), icon: 'i-lucide-copy', color: 'success' })
}
</script>

<template>
  <div class="mx-auto grid w-full max-w-3xl content-start gap-4 px-4 py-8">
    <div class="grid gap-1">
      <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.release.title') }}</h1>
      <p class="text-sm text-muted">{{ t('pages.release.intro') }}</p>
    </div>
    <form class="flex flex-wrap items-end gap-3" @submit.prevent="make">
      <UFormField :label="t('pages.release.from')">
        <USelectMenu v-model="from" :items="fromItems" value-key="value" class="w-52" :ui="{ base: 'font-mono' }" />
      </UFormField>
      <UFormField :label="t('pages.release.to')">
        <USelectMenu v-model="to" :items="toItems" class="w-52" :ui="{ base: 'font-mono' }" />
      </UFormField>
      <UButton type="submit" :label="text ? t('pages.release.again') : t('pages.release.make')" icon="i-lucide-scroll-text" :loading="loading" :disabled="!to" />
    </form>
    <p v-if="!tags.length" class="text-sm text-muted">{{ t('pages.release.noTags') }}</p>
    <template v-if="text">
      <div class="flex items-center gap-2">
        <p class="flex-1 text-xs text-muted">{{ from === FIRST ? t('pages.release.countFromFirst', { count }) : t('pages.release.count', { count }) }}</p>
        <UButton :label="t('common.copy')" icon="i-lucide-copy" size="sm" color="neutral" variant="outline" @click="copy" />
      </div>
      <UTextarea v-model="text" :rows="20" autoresize :maxrows="40" class="w-full" :ui="{ base: 'font-mono text-xs' }" :aria-label="t('pages.release.title')" />
    </template>
  </div>
</template>
