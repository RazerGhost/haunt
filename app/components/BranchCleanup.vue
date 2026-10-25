<script setup lang="ts">
/*
 * Cleaning up branches: the local branches that are merged into the current one (deleting them loses nothing) and the
 * ones whose remote branch is gone (often merged on the remote and deleted there). The merged ones are ticked; a gone
 * one says how many commits only it has, and is deleted only when ticked.
 */
const open = defineModel<boolean>('open', { default: false })
const { repo } = useRepositories()
const { refs } = useRefs()
const toast = useToast()

interface Candidate { name: string, reason: 'merged' | 'gone', only: number }
const candidates = ref<Candidate[]>([])
const ticked = ref(new Set<string>())
const loading = ref(false)
const deleting = ref(false)

watch(open, async (now) => {
  if (!now || !repo.value) return
  loading.value = true
  try {
    const at = repo.value
    const merged = await haunt.mergedBranches(at)
    const gone = (refs.value?.branches ?? []).filter(branch => branch.gone && !branch.current && !merged.includes(branch.name)).map(branch => branch.name)
    const only = await Promise.all(gone.map(name => haunt.unmerged(at, name).catch(() => 0)))
    candidates.value = [
      ...merged.map(name => ({ name, reason: 'merged' as const, only: 0 })),
      ...gone.map((name, index) => ({ name, reason: 'gone' as const, only: only[index] ?? 0 }))
    ]
    ticked.value = new Set(merged)
  } finally {
    loading.value = false
  }
})
function toggle(name: string): void {
  const next = new Set(ticked.value)
  if (next.has(name)) next.delete(name)
  else next.add(name)
  ticked.value = next
}

async function remove(): Promise<void> {
  const at = repo.value
  if (!at || !ticked.value.size) return
  deleting.value = true
  const failed: string[] = []
  for (const candidate of candidates.value.filter(item => ticked.value.has(item.name))) {
    try {
      await haunt.deleteBranch(at, candidate.name, candidate.only > 0)
    } catch {
      failed.push(candidate.name)
    }
  }
  deleting.value = false
  const done = ticked.value.size - failed.length
  toast.add({
    title: `Deleted ${done} ${done === 1 ? 'branch' : 'branches'}`,
    description: failed.length ? `Not deleted: ${failed.join(', ')}.` : 'git keeps their commits for a while in its reflog.',
    color: failed.length ? 'warning' : 'success',
    icon: 'i-lucide-check'
  })
  open.value = false
}
</script>

<template>
  <UModal v-model:open="open" title="Clean up branches" description="Merged branches, and branches whose remote is gone.">
    <template #body>
      <div v-if="loading" class="grid justify-items-center py-6"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
      <p v-else-if="!candidates.length" class="text-ui text-muted">Nothing to clean up.</p>
      <ul v-else class="grid gap-1">
        <li v-for="candidate in candidates" :key="candidate.name">
          <label class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 hover:bg-elevated">
            <UCheckbox :model-value="ticked.has(candidate.name)" @update:model-value="toggle(candidate.name)" />
            <span class="min-w-0 flex-1 truncate font-mono text-ui">{{ candidate.name }}</span>
            <UBadge v-if="candidate.reason === 'merged'" label="merged" color="success" variant="subtle" size="sm" />
            <UBadge v-else :label="candidate.only ? `remote gone · ${candidate.only} ${candidate.only === 1 ? 'commit' : 'commits'} only here` : 'remote gone'" :color="candidate.only ? 'warning' : 'neutral'" variant="subtle" size="sm" />
          </label>
        </li>
      </ul>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton label="Cancel" color="neutral" variant="ghost" @click="open = false" />
        <UButton :label="`Delete ${ticked.size} ${ticked.size === 1 ? 'branch' : 'branches'}`" color="error" variant="soft" :disabled="!ticked.size || deleting" :loading="deleting" @click="remove" />
      </div>
    </template>
  </UModal>
</template>
