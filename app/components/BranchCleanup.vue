<script setup lang="ts">
/*
 * Cleaning up branches: the local branches that are merged into the current one (deleting them loses nothing), the
 * ones the main line already has in full under other shas (squash-merged or rebased in: useLanded), and the ones whose
 * remote branch is gone (often merged on the remote and deleted there). The merged and landed ones are ticked; a gone
 * one says how many commits only it has, and is deleted only when ticked.
 */
const open = defineModel<boolean>('open', { default: false })
const { repo } = useRepositories()
const { refs } = useRefs()
const toast = useToast()
const landed = useLanded()

interface Candidate { name: string, reason: 'merged' | 'landed' | 'gone', only: number }
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
    const local = (refs.value?.branches ?? []).filter(branch => !branch.current && !merged.includes(branch.name))
    const inBase = local.filter(branch => landed.whole.value.some(one => one.reference === `refs/heads/${branch.name}`)).map(branch => branch.name)
    const gone = local.filter(branch => branch.gone && !inBase.includes(branch.name)).map(branch => branch.name)
    const only = await Promise.all([...inBase, ...gone].map(name => haunt.unmerged(at, name).catch(() => 0)))
    candidates.value = [
      ...merged.map(name => ({ name, reason: 'merged' as const, only: 0 })),
      ...inBase.map((name, index) => ({ name, reason: 'landed' as const, only: only[index] ?? 0 })),
      ...gone.map((name, index) => ({ name, reason: 'gone' as const, only: only[inBase.length + index] ?? 0 }))
    ]
    ticked.value = new Set([...merged, ...inBase])
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
    title: t('BranchCleanup.deleted', { count: done }),
    description: failed.length ? t('BranchCleanup.notDeleted', { names: failed.join(', ') }) : t('BranchCleanup.reflog'),
    color: failed.length ? 'warning' : 'success',
    icon: 'i-lucide-check'
  })
  open.value = false
}
</script>

<template>
  <UModal v-model:open="open" :title="t('BranchCleanup.title')" :description="t('BranchCleanup.description')">
    <template #body>
      <div v-if="loading" class="grid justify-items-center py-6"><UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" /></div>
      <p v-else-if="!candidates.length" class="text-ui text-muted">{{ t('BranchCleanup.nothing') }}</p>
      <ul v-else class="grid gap-1">
        <li v-for="candidate in candidates" :key="candidate.name">
          <label class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 hover:bg-elevated">
            <UCheckbox :model-value="ticked.has(candidate.name)" @update:model-value="toggle(candidate.name)" />
            <span class="min-w-0 flex-1 truncate font-mono text-ui">{{ candidate.name }}</span>
            <UBadge v-if="candidate.reason === 'merged'" :label="t('BranchCleanup.merged')" color="success" variant="subtle" size="sm" />
            <UBadge v-else-if="candidate.reason === 'landed'" :label="t('BranchCleanup.landed', { base: landed.base.value ?? 'main' })" :title="t('BranchCleanup.landedHelp')" color="success" variant="subtle" size="sm" />
            <UBadge v-else :label="candidate.only ? t('BranchCleanup.goneOnly', { count: candidate.only }) : t('BranchCleanup.gone')" :color="candidate.only ? 'warning' : 'neutral'" variant="subtle" size="sm" />
          </label>
        </li>
      </ul>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton :label="t('common.cancel')" color="neutral" variant="ghost" @click="open = false" />
        <UButton :label="t('BranchCleanup.delete', { count: ticked.size })" color="error" variant="soft" :disabled="!ticked.size || deleting" :loading="deleting" @click="remove" />
      </div>
    </template>
  </UModal>
</template>
