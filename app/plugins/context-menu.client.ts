/*
 * The webview's own right-click menu (Back, Refresh, Save as, Print) has nothing for Haunt, and Refresh loses the
 * window's state: in the app it is kept only for text fields and selected text, for Cut, Copy and Paste. Haunt's own
 * menus (UContextMenu) prevent it themselves. While Haunt is worked on it stays, for the developer tools' Inspect.
 */
export default defineNuxtPlugin(() => {
  if (import.meta.dev || !useDesktop().inApp) return
  document.addEventListener('contextmenu', (event) => {
    if (event.defaultPrevented) return
    const target = event.target instanceof Element ? event.target : null
    const editable = target?.closest('input, textarea, [contenteditable]:not([contenteditable="false"])')
    const selected = !!window.getSelection()?.toString()
    if (!editable && !selected) event.preventDefault()
  })
})
