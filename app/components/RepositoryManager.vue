<script setup lang="ts">
/*
 * The repository manager, in place of an open repository when none is open or the + tab asks for another: on the left
 * the recent repositories and those in your repository folders (RootPicker), on the right the one picked, at a glance
 * (RepoOverview). Picking one shows it; opening it puts it in a tab.
 */
const { info } = useRepositories()
const picked = ref<string | null>(info.value?.recent[0] ?? null)
watch(() => info.value?.recent[0], (first) => {
  if (!picked.value && first) picked.value = first
})
</script>

<template>
  <div class="grid h-full min-h-0 grid-cols-[22rem_minmax(0,1fr)]">
    <RootPicker v-model:picked="picked" />
    <RepoOverview v-if="picked" :key="picked" :path="picked" />
    <div v-else class="grid place-items-center p-8 text-sm text-muted">{{ t('RepositoryManager.nothingPicked') }}</div>
  </div>
</template>
