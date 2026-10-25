<script setup lang="ts">
import type { Window } from '@tauri-apps/api/window'

// The desktop window's own buttons (Windows and Linux; the window has no system frame): minimise, maximise, close.
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
  <div class="flex self-stretch">
    <button type="button" :class="button" aria-label="Minimise" @click="appWindow?.minimize()">
      <UIcon name="i-lucide-minus" class="size-4" />
    </button>
    <button type="button" :class="button" :aria-label="maximized ? 'Restore' : 'Maximise'" @click="appWindow?.toggleMaximize()">
      <UIcon :name="maximized ? 'i-lucide-copy' : 'i-lucide-square'" class="size-3.5" />
    </button>
    <button type="button" :class="[button, 'hover:bg-[#c42b1c] hover:text-white focus-visible:bg-[#c42b1c] focus-visible:text-white']" aria-label="Close" @click="appWindow?.close()">
      <UIcon name="i-lucide-x" class="size-4" />
    </button>
  </div>
</template>
