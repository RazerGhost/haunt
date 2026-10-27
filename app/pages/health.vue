<script setup lang="ts">
import type { Health } from '~/bindings'

/*
 * Repository health (src-tauri/src/git/health.rs): how much room the history takes, the files that take the most of it
 * (deleted ones too: the history keeps every version), the files that belong in Git LFS, and branches nobody has
 * touched in a long time. Read only: it says what could be done, and leaves the rewriting tools to you.
 */
const { repo } = useRepositories()
const { refs } = useRefs()
const failure = useFailure()

const health = shallowRef<Health | null>(null)
// A slow answer for the tab before lands nowhere.
const ask = latest(repo)
const { loading } = ask
async function load(): Promise<void> {
  health.value = null
  if (!repo.value) return
  try {
    const found = await ask.run(at => haunt.health(at))
    if (found !== undefined) health.value = found
  } catch (error) {
    failure.failed(t('pages.health.readFailed'), error)
  }
}
watch(repo, () => void load(), { immediate: true })

/** Branches whose last commit is older than this are old. */
const OLD_DAYS = 90
const old = computed(() => {
  const cutoff = Date.now() / 1000 - OLD_DAYS * 86_400
  return (refs.value?.branches ?? []).filter(branch => !branch.current && branch.time < cutoff).sort((a, b) => a.time - b.time)
})
const cleaning = ref(false)
const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString(locale())
const deletedBytes = computed(() => (health.value?.largest ?? []).filter(file => !file.inHead).reduce((sum, file) => sum + file.bytes, 0))
</script>

<template>
  <div class="mx-auto grid w-full max-w-3xl content-start gap-6 px-4 py-8">
    <div class="flex items-start gap-3">
      <div class="grid flex-1 gap-1">
        <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.health.title') }}</h1>
        <p class="text-sm text-muted">{{ t('pages.health.intro') }}</p>
      </div>
      <UButton :label="t('pages.health.readAgain')" icon="i-lucide-refresh-cw" size="sm" color="neutral" variant="ghost" :loading="loading" @click="load" />
    </div>
    <p v-if="loading && !health" class="text-sm text-muted">{{ t('pages.health.reading') }}</p>

    <template v-if="health">
      <section class="grid gap-1">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.health.size') }}</h2>
        <p class="text-2xl font-semibold text-highlighted tabular-nums">{{ formatBytes(health.bytes) }}</p>
        <p class="text-sm text-muted">{{ t('pages.health.packs', { count: health.packs }) }}<template v-if="health.looseObjects">{{ t('pages.health.looseBefore', { count: health.looseObjects }) }}<code>git gc</code>{{ t('pages.health.looseAfter') }}</template>{{ t('pages.health.packsAfter') }}</p>
      </section>

      <section class="grid gap-2">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.health.largest') }}</h2>
        <p class="text-sm text-muted">{{ t('pages.health.everyVersion') }}<template v-if="deletedBytes"> {{ t('pages.health.deletedBefore', { size: formatBytes(deletedBytes) }) }}<code>git filter-repo</code>{{ t('pages.health.deletedAfter') }}</template></p>
        <ul class="grid gap-0.5">
          <li v-for="file in health.largest" :key="file.path" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
            <span class="min-w-0 flex-1 truncate font-mono text-xs" :class="{ 'text-muted line-through': !file.inHead }" :title="file.path">{{ file.path }}</span>
            <UBadge v-if="!file.inHead" :label="t('pages.health.deleted')" size="sm" color="neutral" variant="subtle" />
            <span class="text-xs text-muted tabular-nums">{{ t('pages.health.versions', { count: file.versions }) }}</span>
            <span class="w-20 text-right font-medium tabular-nums">{{ formatBytes(file.bytes) }}</span>
          </li>
        </ul>
      </section>

      <section class="grid gap-2">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.health.forLfs') }}</h2>
        <p v-if="!health.lfs.length" class="text-sm text-muted">{{ health.usesLfs ? t('pages.health.inLfs') : t('pages.health.noLfs') }}</p>
        <template v-else>
          <p class="text-sm text-muted">{{ t('pages.health.lfsBefore') }}<code>git lfs track</code>{{ t('pages.health.lfsMiddle') }}<code>git lfs migrate</code>{{ t('pages.health.lfsAfter') }}</p>
          <ul class="grid gap-0.5">
            <li v-for="file in health.lfs" :key="file.path" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
              <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="file.path">{{ file.path }}</span>
              <span class="w-20 text-right tabular-nums">{{ formatBytes(file.bytes) }}</span>
            </li>
          </ul>
        </template>
      </section>
    </template>

    <section class="grid gap-2">
      <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.health.old') }}</h2>
      <p v-if="!old.length" class="text-sm text-muted">{{ t('pages.health.noOld', { days: OLD_DAYS }) }}</p>
      <template v-else>
        <p class="text-sm text-muted">{{ t('pages.health.oldCount', { count: old.length, days: OLD_DAYS }) }}</p>
        <ul class="grid gap-0.5">
          <li v-for="branch in old" :key="branch.name" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
            <span class="min-w-0 flex-1 truncate">{{ branch.name }}</span>
            <span class="min-w-0 max-w-64 truncate text-xs text-muted">{{ branch.subject }}</span>
            <span class="text-xs text-muted tabular-nums">{{ day(branch.time) }}</span>
          </li>
        </ul>
        <UButton :label="t('pages.health.cleanUp')" icon="i-lucide-brush-cleaning" size="sm" color="neutral" variant="outline" class="justify-self-start" @click="cleaning = true" />
      </template>
    </section>
    <BranchCleanup v-model:open="cleaning" />
  </div>
</template>
