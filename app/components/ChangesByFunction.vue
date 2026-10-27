<script setup lang="ts">
/*
 * Local changes, by function: what the working tree changes compared with a base, and what that could break: the functions
 * that call a changed one, a few steps out. By default the base is where the branch left main, so the branch's
 * commits and the uncommitted work show together.
 */
const { changes, codeFiles, otherFiles, affectedNamed, summary, base, depth, loading, problem, result } = useChanges()
const changed = computed(() => changedIds(codeFiles.value))

const bases = computed(() => [
  { value: 'merge-base', label: t('ChangesByFunction.sinceMain'), icon: 'i-lucide-git-branch' },
  { value: 'uncommitted', label: t('ChangesByFunction.uncommitted'), icon: 'i-lucide-pencil' }
])
const custom = ref(base.value.startsWith('ref:') ? base.value.slice(4) : '')
const choice = computed({
  get: () => (bases.value.some(item => item.value === base.value) ? base.value : 'custom'),
  set: (value: string) => {
    if (value !== 'custom') base.value = value
  }
})
function useCustom() {
  if (custom.value.trim()) base.value = `ref:${custom.value.trim()}`
}

</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-default px-4 py-2 text-sm">
      <USelect
        v-model="choice"
        :items="[...bases, { value: 'custom', label: t('ChangesByFunction.compareWithMenu'), icon: 'i-lucide-git-commit-horizontal' }]"
        size="sm"
        class="w-64"
        :aria-label="t('ChangesByFunction.compareWith')"
      />
      <form v-if="choice === 'custom'" class="flex gap-1.5" @submit.prevent="useCustom">
        <UInput v-model="custom" size="sm" placeholder="v1.2, develop, a1b2c3d" class="w-44" :ui="{ base: 'font-mono' }" :aria-label="t('ChangesByFunction.ref')" />
        <UButton type="submit" :label="t('ChangesByFunction.compare')" size="sm" color="neutral" variant="outline" />
      </form>
      <span v-if="changes" class="text-muted">
        {{ t('ChangesByFunction.comparedWith') }} <b class="text-highlighted">{{ said(changes.base.label) }}</b>
        <span class="font-mono text-xs">({{ changes.base.sha.slice(0, 7) }})</span>
        <template v-if="changes.base.ahead || changes.base.behind"> · {{ t('ChangesByFunction.aheadBehind', { count: changes.base.ahead, behind: changes.base.behind }) }}</template>
      </span>
      <UButton v-if="changes && base === 'merge-base'" to="/review" :label="t('ChangesByFunction.review')" icon="i-lucide-list-checks" size="sm" color="neutral" variant="outline" />
      <div class="ml-auto flex items-center gap-2 text-muted">
        <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-4 animate-spin" />
        {{ t('ChangesByFunction.stepsBefore') }}
        <UFieldGroup size="sm">
          <UButton v-for="step in [1, 2, 3]" :key="step" :label="`${step}`" :color="depth === step ? 'primary' : 'neutral'" :variant="depth === step ? 'solid' : 'outline'" @click="depth = step" />
        </UFieldGroup>
        {{ t('ChangesByFunction.stepsAfter') }}
      </div>
    </div>

    <UAlert v-if="problem" class="m-4" color="error" variant="subtle" icon="i-lucide-circle-alert" :title="t('ChangesByFunction.readError')" :description="problem" />
    <div v-else-if="result && !result.changes" class="grid flex-1 place-items-center p-8 text-center text-muted">
      <div class="grid justify-items-center gap-2">
        <UIcon name="i-lucide-git-branch" class="size-6" />
        <p>{{ t('ChangesByFunction.notRepo') }}</p>
      </div>
    </div>
    <div v-else-if="changes && !changes.files.length" class="grid flex-1 place-items-center p-8 text-center text-muted">
      <div class="grid justify-items-center gap-2">
        <UIcon name="i-lucide-check-circle" class="size-6 text-success" />
        <p>{{ t('ChangesByFunction.nothingSince', { base: said(changes.base.label) }) }}</p>
        <p class="text-xs">{{ t('ChangesByFunction.followsAlong') }}</p>
      </div>
    </div>

    <div v-else-if="changes" class="grid min-h-0 flex-1 gap-0 lg:grid-cols-[minmax(0,1fr)_24rem]">
      <section class="min-h-0 overflow-auto p-4">
        <p class="mb-4 text-sm text-muted tabular-nums">
          {{ t('ChangesByFunction.functionsIn', { count: summary.functions, files: t('ChangesByFunction.files', { count: codeFiles.length }) }) }}
          <span class="text-success">+{{ summary.added }}</span> <span class="text-error">−{{ summary.removed }}</span>
          · {{ t('ChangesByFunction.affected', { count: summary.affected }) }}
        </p>
        <RiskLine :files="codeFiles" class="-mt-2 mb-4" />
        <ChangedFunctions :files="codeFiles" />
        <details v-if="otherFiles.length" class="mt-2 text-sm">
          <summary class="cursor-pointer text-muted">{{ t('ChangesByFunction.otherFiles', { count: otherFiles.length }) }}</summary>
          <ul class="mt-2 grid gap-1 font-mono text-xs">
            <li v-for="file in otherFiles" :key="file.path" class="flex gap-2">
              <span class="w-16 text-muted">{{ t(`ChangesByFunction.status.${file.status}`) }}</span>
              <span class="min-w-0 flex-1 truncate">{{ file.path }}</span>
              <span class="tabular-nums"><span class="text-success">+{{ file.added }}</span> <span class="text-error">−{{ file.removed }}</span></span>
            </li>
          </ul>
        </details>
      </section>

      <aside class="min-h-0 overflow-auto border-t border-default p-4 lg:border-t-0 lg:border-l">
        <TestsReached :changed="changed" class="mb-6" />
        <SuggestedReviewers :base="changes.base.sha" :files="codeFiles" class="mb-6" />
        <h2 class="mb-1 font-semibold text-highlighted">{{ t('ChangesByFunction.impact') }}</h2>
        <p class="mb-3 text-xs text-muted">{{ t('ChangesByFunction.impactHelp', { count: depth }) }}</p>
        <AffectedList :affected="affectedNamed" />
      </aside>
    </div>
    <div v-else class="grid flex-1 place-items-center text-muted">
      <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
    </div>
  </div>
</template>
