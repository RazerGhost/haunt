<script setup lang="ts">
// The scan's state as one button: a dot for live, scanning or a problem, the details in its tooltip; a click rescans.
const { scan, status, message, rescan } = useCode()

const state = computed(() => ({
  loading: { dot: 'bg-(--ui-text-dimmed)', label: 'Loading' },
  scanning: { dot: 'bg-warning', label: 'Scanning' },
  ready: { dot: 'bg-success', label: 'Live' },
  error: { dot: 'bg-error', label: 'Problem' },
  'no-root': { dot: 'bg-(--ui-text-dimmed)', label: 'No project' }
} as const)[status.value])

const details = computed(() => {
  if (message.value) return message.value
  const current = scan.value
  if (!current) return state.value.label
  const at = new Date(current.scannedAt).toLocaleTimeString()
  const calls = current.ms.calls === null ? '' : `, calls ${(current.ms.calls / 1000).toFixed(1)} s`
  return `Live: scanned at ${at} (${(current.ms.structure / 1000).toFixed(1)} s${calls})`
})
</script>

<template>
  <UTooltip :text="`${details}. Click to rescan.`">
    <UButton color="neutral" variant="ghost" size="sm" :aria-label="`${state.label}: rescan`" class="relative" @click="rescan">
      <UIcon name="i-lucide-refresh-cw" class="size-4" :class="{ 'animate-spin': status === 'scanning' }" />
      <span class="absolute top-1 right-1 size-1.5 rounded-full" :class="state.dot" />
    </UButton>
  </UTooltip>
</template>
