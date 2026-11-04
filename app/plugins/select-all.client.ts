/*
 * Ctrl A (⌘ A) selects within the place you are in (placeOf), not the whole window. In a text field it is the field's
 * own; in a place marked data-select-all="text" (a diff, code, a commit's message) it selects that place's text; in a
 * list marked with any other name (the files of Local changes) the list hears the window event `haunt:select-all` with
 * that name and selects its items; anywhere else it selects nothing.
 */
export default defineNuxtPlugin(() => {
  document.addEventListener('keydown', (event) => {
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'a') return
    const target = event.target instanceof Element ? event.target : null
    if (target?.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return
    event.preventDefault()
    const place = placeOf('[data-select-all]')
    // Nothing to select here: a selection made before goes, as nothing is what Ctrl A selects.
    if (!place) return window.getSelection()?.removeAllRanges()
    const name = place.getAttribute('data-select-all')
    if (name === 'text') {
      const range = document.createRange()
      range.selectNodeContents(place)
      const selection = window.getSelection()
      selection?.removeAllRanges()
      selection?.addRange(range)
    } else if (name) {
      window.dispatchEvent(new CustomEvent('haunt:select-all', { detail: name }))
    }
  })
})
