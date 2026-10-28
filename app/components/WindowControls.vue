<script setup lang="ts">
import type { Window } from '@tauri-apps/api/window'

// The desktop window's own buttons (Windows and Linux; the window has no system frame), on one side of the title bar.
// On Linux the desktop's `button-layout` says which and on which side, as GTK apps draw them (desktop.rs). Without
// one: minimise, maximise and close on the right, and in a tiling compositor (Hyprland, Sway…) only Close, since the
// compositor places and sizes windows.
const props = defineProps<{ side: 'left' | 'right' }>()
const { tiling, desktop } = useRepositories()
const shown = computed(() => {
  if (desktop.value.buttons) return desktop.value.buttons[props.side]
  if (props.side === 'left') return []
  return tiling.value ? ['close'] : ['minimize', 'maximize', 'close']
})
const maximized = ref(false)
let appWindow: Window | null = null
let stop: (() => void) | null = null

onMounted(async () => {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  appWindow = getCurrentWindow()
  maximized.value = await appWindow.isMaximized()
  stop = await appWindow.onResized(async () => {
    maximized.value = await appWindow!.isMaximized()
  })
})
onBeforeUnmount(() => stop?.())

const button = 'grid h-10 w-11 place-items-center text-muted transition-colors hover:bg-elevated hover:text-highlighted focus-visible:outline-none focus-visible:bg-elevated'
</script>

<template>
  <div v-if="shown.length" class="flex self-stretch">
    <template v-for="name in shown" :key="name">
      <UTooltip v-if="name === 'minimize'" :text="t('WindowControls.minimise')">
        <button type="button" :class="button" :aria-label="t('WindowControls.minimise')" @click="appWindow?.minimize()">
          <UIcon name="i-lucide-minus" class="size-4" />
        </button>
      </UTooltip>
      <UTooltip v-else-if="name === 'maximize'" :text="maximized ? t('common.restore') : t('WindowControls.maximise')">
        <button type="button" :class="button" :aria-label="maximized ? t('common.restore') : t('WindowControls.maximise')" @click="appWindow?.toggleMaximize()">
          <UIcon :name="maximized ? 'i-lucide-copy' : 'i-lucide-square'" class="size-3.5" />
        </button>
      </UTooltip>
      <UTooltip v-else :text="t('common.close')">
        <button type="button" :class="[button, 'hover:bg-[#c42b1c] hover:text-white focus-visible:bg-[#c42b1c] focus-visible:text-white']" :aria-label="t('common.close')" @click="appWindow?.close()">
          <UIcon name="i-lucide-x" class="size-4" />
        </button>
      </UTooltip>
    </template>
  </div>
</template>
