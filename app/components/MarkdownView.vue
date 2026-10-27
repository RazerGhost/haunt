<script setup lang="ts">
import { type FunctionalComponent, h, type VNode, type VNodeChild } from 'vue'
import { type Align, type Block, type Inline, parseMarkdown } from '~~/shared/markdown'

/*
 * Markdown (a repository's README) shown as elements and text only. The text is untrusted and the window can call
 * Haunt's commands, so nothing here sets HTML: shared/markdown.ts reads it into a plain tree and this renders that
 * tree with h(), where every string is a text node. Images are named, not loaded. A link is not an <a>: clicking it
 * emits `link` with the href (already checked by safeHref) and the parent decides what to do.
 */
const props = defineProps<{ text: string }>()
const emit = defineEmits<{ link: [href: string] }>()

const blocks = computed(() => parseMarkdown(props.text))

const HEADING: Record<number, string> = {
  1: 'border-b border-default pb-1 text-xl font-semibold',
  2: 'border-b border-default pb-1 text-lg font-semibold',
  3: 'text-base font-semibold',
  4: 'text-sm font-semibold',
  5: 'text-sm font-semibold',
  6: 'text-xs font-semibold uppercase tracking-wide'
}
const ALIGN: Record<string, string> = { left: 'text-left', center: 'text-center', right: 'text-right' }
const alignClass = (align: Align | undefined) => (align ? ALIGN[align] : 'text-left')

function inline(nodes: Inline[]): VNodeChild[] {
  return nodes.map((node): VNodeChild => {
    switch (node.kind) {
      case 'text': return node.text
      case 'break': return h('br')
      case 'code': return h('code', { class: 'rounded bg-elevated px-1 py-0.5 font-mono text-[0.85em]' }, node.text)
      case 'strong': return h('strong', { class: 'font-semibold text-highlighted' }, inline(node.children))
      case 'em': return h('em', inline(node.children))
      case 'strike': return h('s', inline(node.children))
      case 'image': return h('span', { class: 'text-xs text-muted' }, node.alt ? t('MarkdownView.image', { alt: node.alt }) : t('MarkdownView.imageNoAlt'))
      case 'link': {
        const href = node.href
        return h('span', {
          role: 'link',
          tabindex: 0,
          title: node.title || href,
          class: 'cursor-pointer text-primary hover:underline focus-visible:underline focus-visible:outline-none',
          onClick: () => emit('link', href),
          onKeydown: (event: KeyboardEvent) => {
            if (event.key !== 'Enter') return
            event.preventDefault()
            emit('link', href)
          }
        }, inline(node.children))
      }
    }
  })
}

function block(node: Block, tight = false): VNode {
  switch (node.kind) {
    case 'heading': return h(`h${node.level}`, { class: `${HEADING[node.level]} text-highlighted` }, inline(node.children))
    case 'paragraph': return h(tight ? 'div' : 'p', inline(node.children))
    case 'code': return h('pre', { class: 'overflow-x-auto rounded-md border border-default bg-elevated p-3 font-mono text-xs leading-relaxed' }, [h('code', node.text)])
    case 'quote': return h('blockquote', { class: 'space-y-3 border-l-2 border-default pl-3 text-muted' }, node.blocks.map(inner => block(inner)))
    case 'rule': return h('hr', { class: 'border-default' })
    case 'list': return h(node.ordered ? 'ol' : 'ul', {
      class: [node.ordered ? 'list-decimal' : 'list-disc', 'space-y-1 pl-6 marker:text-muted'],
      ...(node.ordered && node.start !== 1 ? { start: node.start } : {})
    }, node.items.map(item => h('li', {
      class: [node.tight ? 'space-y-1' : 'space-y-3', item.checked !== undefined && 'list-none -ml-5']
    }, [
      ...(item.checked !== undefined
        ? [h('input', { type: 'checkbox', checked: item.checked, disabled: true, class: 'mr-1.5 align-middle', 'aria-label': item.checked ? t('MarkdownView.done') : t('MarkdownView.notDone') })]
        : []),
      // A checkbox and its first paragraph share a line.
      ...item.blocks.map((inner, at) => (at === 0 && item.checked !== undefined && inner.kind === 'paragraph'
        ? h('span', inline(inner.children))
        : block(inner, node.tight)))
    ])))
    case 'table': return h('div', { class: 'overflow-x-auto' }, [
      h('table', { class: 'border-collapse text-sm' }, [
        h('thead', [h('tr', node.header.map((cell, column) => h('th', { class: ['border border-default bg-elevated px-2 py-1 font-semibold text-highlighted', alignClass(node.align[column])] }, inline(cell))))]),
        h('tbody', node.rows.map(row => h('tr', row.map((cell, column) => h('td', { class: ['border border-default px-2 py-1 align-top', alignClass(node.align[column])] }, inline(cell))))))
      ])
    ])
  }
}

const Blocks: FunctionalComponent<{ blocks: Block[] }> = ({ blocks: list }) => list.map(node => block(node))
Blocks.props = ['blocks']
</script>

<template>
  <div class="space-y-3 text-sm leading-relaxed wrap-break-word text-default">
    <Blocks :blocks="blocks" />
  </div>
</template>
