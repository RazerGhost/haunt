<script setup lang="ts">
/*
 * Finding in a block of text (a diff, code, blame, a commit's message: the places marked data-select-all="text"):
 * Ctrl F there opens this bar over its top right corner (AppToolbar sends `haunt:find`), elsewhere Ctrl F stays the
 * function search. Every match is marked with the CSS Custom Highlight API, so nothing in the text's own elements
 * changes; a match may run across the text's elements (a highlighted token, a line's parts), never into the ones that
 * cannot be selected (line numbers). Enter and Shift Enter go to the next and previous match, Escape closes.
 */
const target = shallowRef<HTMLElement | null>(null)
const query = ref('')
const ranges = shallowRef<Range[]>([])
const at = ref(0)
const box = ref({ top: 0, right: 0 })
const input = useTemplateRef<{ inputRef: HTMLInputElement | null }>('input')
const supported = import.meta.client && 'highlights' in CSS

function open(event: Event): void {
  const place = (event as CustomEvent<HTMLElement>).detail
  target.value = place
  const rect = place.getBoundingClientRect()
  box.value = { top: rect.top + 6, right: window.innerWidth - rect.right + 18 }
  // What is selected in it is what to find, as a browser does.
  const selected = window.getSelection()?.toString().trim()
  if (selected && !selected.includes('\n') && place.contains(window.getSelection()!.anchorNode)) query.value = selected
  nextTick(() => {
    input.value?.inputRef?.focus()
    input.value?.inputRef?.select()
    find()
  })
}
function close(): void {
  target.value = null
  ranges.value = []
  if (supported) {
    CSS.highlights.delete('haunt-find')
    CSS.highlights.delete('haunt-find-current')
  }
}

/** Every match in the target's selectable text, as ranges over its text nodes. */
function find(keepAt = false): void {
  const place = target.value
  const wanted = query.value.toLowerCase()
  if (!place || !place.isConnected || !wanted) {
    ranges.value = []
    paint()
    return
  }
  const nodes: { node: Text, start: number }[] = []
  let text = ''
  const walker = document.createTreeWalker(place, NodeFilter.SHOW_TEXT, {
    acceptNode: node => (node.parentElement?.closest('.select-none') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT)
  })
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    nodes.push({ node, start: text.length })
    text += node.data
  }
  const lower = text.toLowerCase()
  const found: Range[] = []
  // The text node holding offset `offset`, and the offset in it.
  const locate = (offset: number, end: boolean) => {
    let low = 0
    let high = nodes.length - 1
    while (low < high) {
      const middle = (low + high + 1) >> 1
      if (nodes[middle]!.start < offset || (!end && nodes[middle]!.start === offset)) low = middle
      else high = middle - 1
    }
    return { node: nodes[low]!.node, offset: offset - nodes[low]!.start }
  }
  for (let index = lower.indexOf(wanted); index !== -1 && found.length < 2000; index = lower.indexOf(wanted, index + wanted.length)) {
    const from = locate(index, false)
    const to = locate(index + wanted.length, true)
    const range = document.createRange()
    range.setStart(from.node, from.offset)
    range.setEnd(to.node, to.offset)
    found.push(range)
  }
  ranges.value = found
  if (!keepAt) at.value = 0
  at.value = Math.min(at.value, Math.max(0, found.length - 1))
  paint()
  if (!keepAt) show()
}
function paint(): void {
  if (!supported) return
  const current = ranges.value[at.value]
  CSS.highlights.set('haunt-find', new Highlight(...ranges.value.filter(range => range !== current)))
  if (current) CSS.highlights.set('haunt-find-current', new Highlight(current))
  else CSS.highlights.delete('haunt-find-current')
}
function show(): void {
  const range = ranges.value[at.value]
  range?.startContainer.parentElement?.scrollIntoView({ block: 'center', inline: 'nearest' })
}
function step(by: number): void {
  if (!ranges.value.length) return
  at.value = (at.value + by + ranges.value.length) % ranges.value.length
  paint()
  show()
}
function onKey(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    step(event.shiftKey ? -1 : 1)
    event.preventDefault()
  } else if (event.key === 'Escape') {
    close()
    event.preventDefault()
  }
}

watch(query, () => find())
// The text changes under it (a diff read again, a page of blame): found again, on the same match where it can be.
let observer: MutationObserver | null = null
watch(target, (place) => {
  observer?.disconnect()
  if (!place) return
  observer = new MutationObserver(() => {
    if (!place.isConnected) return close()
    find(true)
  })
  observer.observe(place, { childList: true, subtree: true, characterData: true })
})
onMounted(() => window.addEventListener('haunt:find', open))
onBeforeUnmount(() => {
  window.removeEventListener('haunt:find', open)
  observer?.disconnect()
})
// Another screen: the text it found in is gone.
const route = useRoute()
watch(() => route.fullPath, close)
</script>

<template>
  <div
    v-if="target"
    class="fixed z-40 flex items-center gap-1 rounded-md border border-default bg-default p-1 shadow-lg"
    :style="{ top: `${box.top}px`, right: `${box.right}px` }"
    role="search"
  >
    <UInput ref="input" v-model="query" size="xs" :placeholder="t('TextFind.find')" class="w-52" icon="i-lucide-search" :aria-label="t('TextFind.find')" @keydown="onKey" />
    <span class="min-w-14 text-center text-label text-muted tabular-nums">{{ query ? (ranges.length ? `${at + 1} / ${ranges.length}` : t('TextFind.none')) : '' }}</span>
    <UButton icon="i-lucide-chevron-up" size="xs" color="neutral" variant="ghost" :aria-label="t('TextFind.previous')" :title="t('TextFind.previous')" :disabled="!ranges.length" @click="step(-1)" />
    <UButton icon="i-lucide-chevron-down" size="xs" color="neutral" variant="ghost" :aria-label="t('TextFind.next')" :title="t('TextFind.next')" :disabled="!ranges.length" @click="step(1)" />
    <UButton icon="i-lucide-x" size="xs" color="neutral" variant="ghost" :aria-label="t('TextFind.close')" :title="t('TextFind.close')" @click="close" />
  </div>
</template>

<style>
::highlight(haunt-find) {
  background-color: color-mix(in srgb, var(--color-yellow-400, #facc15) 45%, transparent);
  color: inherit;
}
::highlight(haunt-find-current) {
  background-color: var(--color-orange-400, #fb923c);
  color: #000;
}
</style>
