/** Whether the page runs in the desktop app (Tauri), and on macOS, where the window keeps its own traffic lights. */
export function useDesktop() {
  const inApp = import.meta.client && '__TAURI__' in window
  const mac = import.meta.client && /Mac/.test(navigator.userAgent)
  return { inApp, mac }
}
