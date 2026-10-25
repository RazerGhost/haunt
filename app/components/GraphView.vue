<script lang="ts">
/** Cytoscape's extensions are registered once for the app, not once per graph. */
let registered = false
</script>

<script setup lang="ts">
import type { Core, ElementDefinition, LayoutOptions, StylesheetJson } from 'cytoscape'

/*
 * A Cytoscape graph: drag to pan, scroll to zoom, hover a node to see its neighbours. The page builds the elements
 * (with resolved colours: a canvas cannot read CSS variables) and the stylesheet; this draws them and reports clicks.
 */
const props = defineProps<{
  elements: ElementDefinition[]
  stylesheet: () => StylesheetJson
  layout: LayoutOptions
  selected?: string | null
}>()
const emit = defineEmits<{ node: [id: string], open: [id: string], edge: [id: string], background: [] }>()

const container = ref<HTMLElement>()
const colorMode = useColorMode()
let cy: Core | null = null

function markSelected(): void {
  if (!cy) return
  cy.$('.selected').removeClass('selected')
  if (props.selected) cy.getElementById(props.selected).addClass('selected')
}

/** The nodes and edges drawn last, by id: the same ones again are updated in place, keeping your pan and zoom. */
let drawn = ''
const idOf = (element: ElementDefinition) => element.data.id ?? `${element.data.source}>${element.data.target}`

function draw(): void {
  if (!cy) return
  const ids = props.elements.map(idOf).sort().join('\n')
  if (ids === drawn && cy.elements().length) {
    for (const element of props.elements) {
      const found = cy.getElementById(idOf(element))
      found.data(element.data)
      if (element.classes !== undefined) found.classes(element.classes)
    }
    markSelected()
    return
  }
  drawn = ids
  cy.elements().remove()
  cy.add(props.elements)
  cy.layout(props.layout).run()
  markSelected()
  cy.fit(undefined, 40)
  // A big graph fitted whole is too small to read: open at a readable size around the centre node instead.
  if (cy.zoom() < READABLE) {
    cy.zoom(READABLE)
    cy.center(cy.$('node.center').length ? cy.$('node.center') : undefined)
  }
}

// Below this a fitted graph is too small to read; with grouped columns (Calls) only a very large one gets there.
const READABLE = 0.55

// Left before Cytoscape loaded: the graph is not made at all (nothing would destroy it).
let gone = false
onMounted(async () => {
  const [cytoscape, dagre] = await Promise.all([import('cytoscape').then(module => module.default), import('cytoscape-dagre').then(module => module.default)])
  if (gone) return
  // The layered layout (top to bottom, by import direction) for the import graphs; registered once.
  if (!registered) {
    cytoscape.use(dagre)
    registered = true
  }
  cy = cytoscape({ container: container.value, style: props.stylesheet(), minZoom: 0.05, maxZoom: 3 })
  cy.on('tap', 'node', event => emit('node', event.target.id()))
  cy.on('dbltap', 'node', event => emit('open', event.target.id()))
  cy.on('tap', 'edge', event => emit('edge', event.target.id()))
  cy.on('tap', (event) => {
    if (event.target === cy) emit('background')
  })
  cy.on('mouseover', 'node', (event) => {
    cy!.elements().addClass('faded')
    event.target.closedNeighborhood().removeClass('faded')
  })
  cy.on('mouseout', 'node', () => cy!.elements().removeClass('faded'))
  draw()
})

watch(() => props.elements, draw)
watch(() => props.selected, markSelected)
// The theme's colours live in CSS; restyle once the page has switched.
watch(() => colorMode.value, () => nextTick(() => cy?.style(props.stylesheet())))
onBeforeUnmount(() => {
  gone = true
  cy?.destroy()
  cy = null
})

defineExpose({ fit: () => cy?.fit(undefined, 40) })
</script>

<template>
  <div ref="container" class="size-full min-h-80" />
</template>
