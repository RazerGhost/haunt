<script setup lang="ts">
// Under the toolbar while a bisect is going on: the commit to test and about how many steps are left, with Good, Bad
// and Skip; or, once found, the first bad commit. The Bisect screen has the rest (useBisect).
const { state, busy, mark, stop } = useBisect()
const route = useRoute()
</script>

<template>
  <div v-if="state?.active" class="flex items-center gap-3 border-b border-info/30 bg-info/10 px-3 py-1.5 text-ui" role="status">
    <UIcon name="i-lucide-crosshair" class="size-4 shrink-0 text-info" />
    <p v-if="state.culprit" class="min-w-0 flex-1 truncate">
      <span class="font-medium text-highlighted">Found the first bad commit:</span>
      <span class="text-muted"> {{ state.culprit.subject }} · {{ state.culprit.sha.slice(0, 7) }}</span>
    </p>
    <p v-else class="min-w-0 flex-1 truncate">
      <span class="font-medium text-highlighted">Bisecting</span>
      <span class="text-muted"> · testing {{ state.current?.subject ?? '…' }} · {{ state.left }} {{ state.left === 1 ? 'commit' : 'commits' }} in play, about {{ state.steps }} {{ state.steps === 1 ? 'step' : 'steps' }} left</span>
    </p>
    <template v-if="!state.culprit">
      <UButton label="Good" size="xs" color="success" variant="soft" :loading="busy" @click="mark('good')" />
      <UButton label="Bad" size="xs" color="error" variant="soft" :disabled="busy" @click="mark('bad')" />
      <UButton label="Skip" size="xs" color="neutral" variant="ghost" :disabled="busy" title="This one cannot be tested: try another" @click="mark('skip')" />
    </template>
    <UButton v-else label="See it" size="xs" color="primary" :to="{ path: '/commits', query: { commit: state.culprit.sha } }" />
    <UButton v-if="route.path !== '/bisect'" label="Open" size="xs" color="neutral" variant="ghost" to="/bisect" />
    <UButton label="Stop" size="xs" color="neutral" variant="ghost" :disabled="busy" title="Back to the branch it started from" @click="stop" />
  </div>
</template>
