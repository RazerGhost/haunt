<script setup lang="ts">
import type { ChangedFile } from '~/bindings'

/*
 * Applying patches (usePatches): pick patch files (a commit's menu saves them), see what they would change function by
 * function, worked out on a copy without touching your files, then commit them (`git am`) or put them in the working
 * tree only (`git apply`). Either all apply or none.
 */
const { repo } = useRepositories()
const { files, preview, busy, choose, read, apply, clear } = usePatches()
const failure = useFailure()

const changes = shallowRef<ChangedFile[]>([])
watch([preview, repo], async ([now, at]) => {
  changes.value = []
  if (!now?.commit || !at) return
  try {
    changes.value = (await haunt.range(at, 'HEAD', now.commit)).files
  } catch (error) {
    failure.failed(t('pages.patches.notByFunction'), error, t('pages.patches.previewNotRead'))
  }
}, { immediate: true })
// Another tab: its own patches.
watch(repo, () => clear())
const name = (path: string) => path.split(/[\\/]/).pop() ?? path
</script>

<template>
  <div class="mx-auto grid w-full max-w-3xl content-start gap-4 px-4 py-8">
    <div class="grid gap-1">
      <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.patches.title') }}</h1>
      <p class="text-sm text-muted">{{ t('pages.patches.introBefore') }} <code>git format-patch</code> {{ t('pages.patches.introAfter') }}</p>
    </div>
    <div class="flex flex-wrap gap-2">
      <UButton :label="files.length ? t('pages.patches.chooseOther') : t('pages.patches.choose')" icon="i-lucide-file-diff" :loading="busy && !preview" @click="choose" />
      <UButton v-if="files.length" :label="t('pages.patches.readAgain')" icon="i-lucide-refresh-cw" color="neutral" variant="ghost" :disabled="busy" @click="read" />
    </div>

    <template v-if="preview">
      <section class="grid gap-1">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.patches.patches', { count: preview.patches.length }) }}</h2>
        <ul class="grid gap-1">
          <li v-for="patch in preview.patches" :key="patch.file" class="grid gap-0.5 rounded border border-default px-3 py-2">
            <span class="text-sm text-highlighted">{{ patch.subject }}</span>
            <span class="text-xs text-muted">{{ patch.author || t('pages.patches.noAuthor') }} · {{ name(patch.file) }} · {{ t('pages.patches.files', { count: patch.paths.length }) }}</span>
          </li>
        </ul>
      </section>

      <p v-if="preview.problem" class="flex items-start gap-1.5 rounded border border-error/40 bg-error/5 p-3 text-sm text-error">
        <UIcon name="i-lucide-circle-x" class="mt-0.5 size-4 shrink-0" /><span class="whitespace-pre-wrap">{{ preview.problem }} {{ t('pages.patches.doesNotApply') }}</span>
      </p>
      <template v-else>
        <section class="grid gap-2">
          <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.patches.whatChanges') }}</h2>
          <RiskLine v-if="changes.length" :files="changes" />
          <ChangedFunctions :files="changes" />
        </section>
        <div class="flex flex-wrap gap-2">
          <UButton :label="t('pages.patches.commit')" icon="i-lucide-git-commit-horizontal" :loading="busy" :title="t('pages.patches.commitHelp')" @click="apply(true)" />
          <UButton :label="t('pages.patches.applyOnly')" color="neutral" variant="outline" :disabled="busy" :title="t('pages.patches.applyOnlyHelp')" @click="apply(false)" />
        </div>
      </template>
    </template>
  </div>
</template>
