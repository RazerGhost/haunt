/*
 * Dragging a branch onto another in the sidebar: a branch dropped on the current branch is merged into it, and the
 * current branch dropped on another is rebased onto it (the rebase screen opens to check its commits first). Built on
 * pointer events, not HTML drag and drop: the window takes dropped folders natively, which turns HTML drag and drop off
 * in WebView2.
 */

/** The ref being dragged and the one under the pointer, shared by every branch list in the sidebar. */
const dragged = ref<{ name: string, current: boolean } | null>(null)
const over = ref<string | null>(null)
let start: { x: number, y: number, name: string, current: boolean } | null = null
let ghost: HTMLElement | null = null
/** Set after a drag so the click that ends it does not open the branch's commits. */
let swallowClick = false

export function useRefDrag() {
  const { current } = useRefs()
  const operation = useOperation()
  const toast = useToast()

  function targetAt(x: number, y: number): { name: string, current: boolean } | null {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-ref]')
    return element ? { name: element.dataset.ref!, current: element.dataset.current === 'true' } : null
  }

  function move(event: PointerEvent): void {
    if (!start) return
    if (!dragged.value) {
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < 6) return
      dragged.value = { name: start.name, current: start.current }
      ghost = document.createElement('div')
      ghost.textContent = start.name
      ghost.className = 'pointer-events-none fixed z-50 rounded-md border border-default bg-default px-2 py-0.5 text-ui shadow-md font-mono'
      document.body.append(ghost)
      document.body.style.cursor = 'grabbing'
    }
    ghost!.style.left = `${event.clientX + 12}px`
    ghost!.style.top = `${event.clientY + 8}px`
    const target = targetAt(event.clientX, event.clientY)
    over.value = target && target.name !== dragged.value.name ? target.name : null
  }

  function finish(event: PointerEvent): void {
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', finish)
    const from = dragged.value
    start = null
    dragged.value = null
    over.value = null
    ghost?.remove()
    ghost = null
    document.body.style.cursor = ''
    if (!from) return
    swallowClick = true
    setTimeout(() => { swallowClick = false }, 0)
    const target = targetAt(event.clientX, event.clientY)
    if (!target || target.name === from.name) return
    if (target.current) void operation.merge(from.name)
    else if (from.current) void navigateTo({ path: '/rebase', query: { onto: target.name } })
    else {
      toast.add({
        title: t('composables.useRefDrag.nothing'),
        description: t('composables.useRefDrag.nothingHelp', { current: current.value?.name ?? t('composables.useRefDrag.currentBranch') }),
        icon: 'i-lucide-info'
      })
    }
  }

  /** On a branch's pointerdown: a drag starts once the pointer moves a few pixels. */
  function down(event: PointerEvent, name: string, isCurrent: boolean): void {
    if (event.button !== 0) return
    start = { x: event.clientX, y: event.clientY, name, current: isCurrent }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', finish)
  }

  /** On a branch's click (capture): true when it ended a drag, so the click is ignored. */
  function endedDrag(event: MouseEvent): boolean {
    if (!swallowClick) return false
    event.preventDefault()
    event.stopPropagation()
    return true
  }

  /** What dropping the dragged branch on `name` would do, for the drop target's hint. */
  function hint(name: string, isCurrent: boolean): string | null {
    const from = dragged.value
    if (!from || from.name === name) return null
    if (isCurrent) return t('composables.useRefDrag.merge', { from: from.name, into: name })
    if (from.current) return t('composables.useRefDrag.rebase', { from: from.name, onto: name })
    return null
  }

  return { dragged, over, down, endedDrag, hint }
}
