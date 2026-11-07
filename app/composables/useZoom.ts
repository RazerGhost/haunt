/*
 * The window's zoom, as in a browser: Ctrl + (or Ctrl =) zooms in, Ctrl - out, Ctrl 0 back to 100%, also while typing.
 * The whole page is scaled (Tauri's webview zoom; CSS zoom in a plain browser, while Haunt is worked on), and the level
 * is kept for the next start. A short toast says the level. plugins/haunt.client.ts starts it.
 */

/** The levels it steps through, as in a browser. */
const LEVELS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2]

const level = usePersisted<number>('zoom', 1, value => typeof value === 'number' && LEVELS.includes(value))

async function apply(scale: number): Promise<void> {
  if ('__TAURI_INTERNALS__' in window) {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview')
      await getCurrentWebview().setZoom(scale)
      return
    } catch { /* a webview that cannot: as in a browser, then */ }
  }
  document.documentElement.style.setProperty('zoom', scale === 1 ? '' : String(scale))
}

/** The level `steps` away from `from` (the nearest level first, for one not in the list). */
export function zoomStep(from: number, steps: number): number {
  const at = LEVELS.reduce((best, value, index) => Math.abs(value - from) < Math.abs(LEVELS[best]! - from) ? index : best, 0)
  return LEVELS[Math.min(LEVELS.length - 1, Math.max(0, at + steps))]!
}

export function useZoom() {
  const toast = useToast()
  function set(next: number): void {
    if (next === level.value) return
    level.value = next
    void apply(next)
    toast.remove('zoom')
    toast.add({ id: 'zoom', title: t('composables.useZoom.level', { percent: Math.round(next * 100) }), icon: 'i-lucide-zoom-in', duration: 1500 })
  }
  return {
    level,
    zoomIn: () => set(zoomStep(level.value, 1)),
    zoomOut: () => set(zoomStep(level.value, -1)),
    reset: () => set(1)
  }
}

export function startZoom(): void {
  if (level.value !== 1) void apply(level.value)
  const zoom = useZoom()
  window.addEventListener('keydown', (event) => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return
    const key = event.key
    const action = key === '+' || key === '=' || event.code === 'NumpadAdd'
      ? zoom.zoomIn
      : key === '-' || key === '_' || event.code === 'NumpadSubtract'
        ? zoom.zoomOut
        : (key === '0' || event.code === 'Numpad0') && !event.shiftKey ? zoom.reset : null
    if (!action) return
    event.preventDefault()
    action()
  }, { capture: true })
}
