<script setup lang="ts">
/*
 * Local changes, by function: what the working tree changes compared with a base, and what that could break: the functions
 * that call a changed one, a few steps out. By default the base is where the branch left main, so the branch's
 * commits and the uncommitted work show together.
 */
const { changes, codeFiles, otherFiles, affectedNamed, summary, base, depth, loading, problem, result } = useChanges()
const changed = computed(() => changedIds(codeFiles.value))

const bases = [
  { value: 'merge-base', label: 'Since main', icon: 'i-lucide-git-branch' },
  { value: 'uncommitted', label: 'Uncommitted only', icon: 'i-lucide-pencil' }
]
const custom = ref(base.value.startsWith('ref:') ? base.value.slice(4) : '')
const choice = computed({
  get: () => (bases.some(item => item.value === base.value) ? base.value : 'custom'),
  set: (value: string) => {
    if (value !== 'custom') base.value = value
  }
})
function useCustom() {
  if (custom.value.trim()) base.value = `ref:${custom.value.trim()}`
}

const FILE_STATUS = { added: 'added', modified: 'modified', deleted: 'deleted', renamed: 'renamed' } as const
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-default px-4 py-2 text-sm">
      <USelect
        v-model="choice"
        :items="[...bases, { value: 'custom', label: 'Compare with…', icon: 'i-lucide-git-commit-horizontal' }]"
        size="sm"
        class="w-64"
        aria-label="Compare with"
      />
      <form v-if="choice === 'custom'" class="flex gap-1.5" @submit.prevent="useCustom">
        <UInput v-model="custom" size="sm" placeholder="v1.2, develop, a1b2c3d" class="w-44" :ui="{ base: 'font-mono' }" aria-label="Branch, tag or commit" />
        <UButton type="submit" label="Compare" size="sm" color="neutral" variant="outline" />
      </form>
      <span v-if="changes" class="text-muted">
        Compared with <b class="text-highlighted">{{ changes.base.label }}</b>
        <span class="font-mono text-xs">({{ changes.base.sha.slice(0, 7) }})</span>
        <template v-if="changes.base.ahead || changes.base.behind"> · {{ changes.base.ahead }} commits ahead, {{ changes.base.behind }} behind</template>
      </span>
      <UButton v-if="changes && base === 'merge-base'" to="/review" label="Review" icon="i-lucide-list-checks" size="sm" color="neutral" variant="outline" />
      <div class="ml-auto flex items-center gap-2 text-muted">
        <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-4 animate-spin" />
        Callers
        <UFieldGroup size="sm">
          <UButton v-for="step in [1, 2, 3]" :key="step" :label="`${step}`" :color="depth === step ? 'primary' : 'neutral'" :variant="depth === step ? 'solid' : 'outline'" @click="depth = step" />
        </UFieldGroup>
        steps out
      </div>
    </div>

    <UAlert v-if="problem" class="m-4" color="error" variant="subtle" icon="i-lucide-circle-alert" title="The changes could not be read" :description="problem" />
    <div v-else-if="result && !result.changes" class="grid flex-1 place-items-center p-8 text-center text-muted">
      <div class="grid justify-items-center gap-2">
        <UIcon name="i-lucide-git-branch" class="size-6" />
        <p>This folder is not in a git repository.</p>
      </div>
    </div>
    <div v-else-if="changes && !changes.files.length" class="grid flex-1 place-items-center p-8 text-center text-muted">
      <div class="grid justify-items-center gap-2">
        <UIcon name="i-lucide-check-circle" class="size-6 text-success" />
        <p>Nothing changed since {{ changes.base.label }}.</p>
        <p class="text-xs">Save a file or switch the comparison above; this view follows along.</p>
      </div>
    </div>

    <div v-else-if="changes" class="grid min-h-0 flex-1 gap-0 lg:grid-cols-[minmax(0,1fr)_24rem]">
      <section class="min-h-0 overflow-auto p-4">
        <p class="mb-4 text-sm text-muted tabular-nums">
          {{ summary.functions }} functions in {{ codeFiles.length }} files
          <span class="text-success">+{{ summary.added }}</span> <span class="text-error">−{{ summary.removed }}</span>
          · {{ summary.affected }} callers could be affected
        </p>
        <RiskLine :files="codeFiles" class="-mt-2 mb-4" />
        <ChangedFunctions :files="codeFiles" />
        <details v-if="otherFiles.length" class="mt-2 text-sm">
          <summary class="cursor-pointer text-muted">{{ otherFiles.length }} other files changed (not code Haunt reads)</summary>
          <ul class="mt-2 grid gap-1 font-mono text-xs">
            <li v-for="file in otherFiles" :key="file.path" class="flex gap-2">
              <span class="w-16 text-muted">{{ FILE_STATUS[file.status] }}</span>
              <span class="min-w-0 flex-1 truncate">{{ file.path }}</span>
              <span class="tabular-nums"><span class="text-success">+{{ file.added }}</span> <span class="text-error">−{{ file.removed }}</span></span>
            </li>
          </ul>
        </details>
      </section>

      <aside class="min-h-0 overflow-auto border-t border-default p-4 lg:border-t-0 lg:border-l">
        <TestsReached :changed="changed" class="mb-6" />
        <SuggestedReviewers :base="changes.base.sha" :files="codeFiles" class="mb-6" />
        <h2 class="mb-1 font-semibold text-highlighted">What could break</h2>
        <p class="mb-3 text-xs text-muted">Functions that call a changed one, up to {{ depth }} steps out, nearest first. Calls through interfaces or markup are not included.</p>
        <AffectedList :affected="affectedNamed" />
      </aside>
    </div>
    <div v-else class="grid flex-1 place-items-center text-muted">
      <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
    </div>
  </div>
</template>
