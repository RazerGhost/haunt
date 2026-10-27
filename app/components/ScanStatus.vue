<script setup lang="ts">
// The scan's state as one button: a dot for live, scanning or a problem, the details in its tooltip; a click rescans.
const { scan, status, message, rescan } = useCode()

const state = computed(() => ({
  loading: { dot: 'bg-(--ui-text-dimmed)', label: t('ScanStatus.loading') },
  scanning: { dot: 'bg-warning', label: t('ScanStatus.scanning') },
  ready: { dot: 'bg-success', label: t('ScanStatus.live') },
  error: { dot: 'bg-error', label: t('ScanStatus.problem') },
  'no-root': { dot: 'bg-(--ui-text-dimmed)', label: t('ScanStatus.noProject') }
} as const)[status.value])

const details = computed(() => {
  if (message.value) return message.value
  const current = scan.value
  if (!current) return state.value.label
  const at = new Date(current.scannedAt).toLocaleTimeString()
  const structure = (current.ms.structure / 1000).toFixed(1)
  return current.ms.calls === null
    ? t('ScanStatus.scannedAt', { at, structure })
    : t('ScanStatus.scannedAtCalls', { at, structure, calls: (current.ms.calls / 1000).toFixed(1) })
})
</script>

<template>
  <UTooltip :text="t('ScanStatus.tooltip', { details })">
    <UButton color="neutral" variant="ghost" size="sm" :aria-label="t('ScanStatus.rescan', { state: state.label })" class="relative" @click="rescan">
      <UIcon name="i-lucide-refresh-cw" class="size-4" :class="{ 'animate-spin': status === 'scanning' }" />
      <span class="absolute top-1 right-1 size-1.5 rounded-full" :class="state.dot" />
    </UButton>
  </UTooltip>
</template>
