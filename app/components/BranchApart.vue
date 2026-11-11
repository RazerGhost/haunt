<script setup lang="ts">
/*
 * Above the commit list, when a branch and its remote branch went far apart (wentApart): how far, how many of its own
 * commits the remote already has in another form (useLanded; then its history was most likely rewritten), and the safe
 * next steps: list the commits only here, keep a copy and match the remote branch, or rebase onto it.
 */
const props = defineProps<{ name: string }>()
const { refs } = useRefs()
const landed = useLanded()
const refActions = useRefActions()

const branch = computed(() => refs.value?.branches.find(one => one.name === props.name) ?? null)
const shown = computed(() => !!branch.value && wentApart(branch.value))
/** How many of its own commits the remote branch has under other shas: known when the remote branch is the main line. */
const already = computed(() => {
  const at = branch.value
  if (!at || landed.base.value !== at.upstream) return 0
  return landed.branch(at.name)?.landed ?? 0
})
const hidden = ref(new Set<string>())
const key = computed(() => `${props.name}:${branch.value?.ahead}:${branch.value?.behind}`)
</script>

<template>
  <UAlert
    v-if="shown && branch && !hidden.has(key)"
    class="m-2 mb-0 shrink-0"
    color="warning"
    variant="subtle"
    icon="i-lucide-triangle-alert"
    :title="t('BranchApart.title', { name: branch.name, upstream: branch.upstream! })"
    :close="true"
    @update:open="hidden = new Set([...hidden, key])"
  >
    <template #description>
      <p>
        {{ t('BranchApart.counts', { ahead: branch.ahead, behind: branch.behind, upstream: branch.upstream! }) }}
        {{ already ? t('BranchApart.rewritten', { count: already, ahead: branch.ahead, upstream: branch.upstream! }) : t('BranchApart.pullWould', { upstream: branch.upstream! }) }}
      </p>
      <div class="mt-2 flex flex-wrap gap-1.5">
        <UButton :label="t('BranchApart.onlyHere', { count: branch.ahead })" icon="i-lucide-list" size="xs" color="neutral" variant="outline" :to="{ path: '/commits', query: { from: branch.name, not: branch.upstream! } }" />
        <UButton :label="t('BranchApart.match', { upstream: branch.upstream! })" icon="i-lucide-copy" size="xs" color="neutral" variant="outline" :title="t('BranchApart.matchHelp', { name: branch.name })" @click="refActions.matchUpstream(branch)" />
        <UButton v-if="branch.current" :label="t('BranchApart.rebase', { upstream: branch.upstream! })" icon="i-lucide-git-pull-request-arrow" size="xs" color="neutral" variant="ghost" :to="{ path: '/rebase', query: { onto: branch.upstream! } }" />
      </div>
    </template>
  </UAlert>
</template>
