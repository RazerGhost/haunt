<script setup lang="ts">
// Under the toolbar while a merge, revert, cherry-pick or rebase has not finished: what it is, how many files still
// conflict, and the way on (resolve, continue) or back (abort).
const { operation, busy, proceed, abort } = useOperation()
const route = useRoute()
const left = computed(() => operation.value?.conflicted.length ?? 0)
</script>

<template>
  <div v-if="operation" class="flex items-center gap-3 border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-ui" role="status">
    <UIcon name="i-lucide-git-merge" class="size-4 shrink-0 text-warning" />
    <p class="min-w-0 flex-1 truncate">
      <span class="font-medium text-highlighted">{{ operation.title }}</span>
      <span class="text-muted"> · {{ left ? t('OperationBanner.conflicts', { count: left }) : t('OperationBanner.noConflicts') }}</span>
    </p>
    <UButton v-if="left && route.path !== '/conflicts'" :label="t('OperationBanner.resolve')" size="xs" color="warning" to="/conflicts" />
    <UButton v-if="!left" :label="t('OperationBanner.continue')" size="xs" color="primary" :loading="busy" @click="proceed" />
    <UButton :label="t('OperationBanner.abort')" size="xs" color="neutral" variant="ghost" :disabled="busy" @click="abort" />
  </div>
</template>
