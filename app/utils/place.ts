/*
 * Where you are in the window, for keys that act there (Ctrl A selects within it, Ctrl F finds in it): the element with
 * the keyboard focus, else the one last clicked.
 */
let lastClicked: Element | null = null
if (import.meta.client) {
  document.addEventListener('pointerdown', (event) => {
    lastClicked = event.target instanceof Element ? event.target : null
  }, true)
}

/** The nearest element matching `selector` around where you are, or null. */
export function placeOf(selector: string): HTMLElement | null {
  const focused = document.activeElement !== document.body ? document.activeElement : null
  return focused?.closest<HTMLElement>(selector) ?? (lastClicked?.isConnected ? lastClicked.closest<HTMLElement>(selector) : null)
}
