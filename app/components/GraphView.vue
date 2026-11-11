<script setup lang="ts">
import type { Core, ElementDefinition, LayoutOptions, NodeCollection, StylesheetJson } from 'cytoscape'

/*
 * A Cytoscape graph: drag to pan, scroll to zoom, hover a node to see its neighbours. The page builds the elements
 * (with resolved colours: a canvas cannot read CSS variables) and the stylesheet; this draws them and reports clicks.
 *
 * It opens on what you can read: with a centre node (Calls), on the centre and the nodes next to it; else the whole
 * graph, at a readable size around the middle when that is too small. Labels too small to read are not drawn
 * (graphStyle's `min-zoomed-font-size`); hovering a node draws its label at a readable size and its neighbours stand out. `fit()`
 * shows everything.
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
  const center = cy.$('node.center')
  if (center.length) {
    // The centre and the nodes it calls or is called by, kept between readable and not oversized.
    cy.fit(center.closedNeighborhood('node'), 40)
    const zoom = Math.min(MAX_OPEN, Math.max(READABLE, cy.zoom()))
    if (zoom !== cy.zoom()) {
      cy.zoom(zoom)
      cy.center(center)
    }
    return
  }
  cy.fit(undefined, 40)
  // A big graph fitted whole is too small to read: open at a readable size around the middle instead.
  if (cy.zoom() < READABLE_WHOLE) {
    cy.zoom(READABLE_WHOLE)
    cy.center()
  }
}

// Around a centre node the graph opens at least this big (an 11px label reads at about 10px), and at most this big.
const READABLE = 0.9
const MAX_OPEN = 1.4
// A whole graph (the import map) opens at least this big: its labels just stay drawn.
const READABLE_WHOLE = 0.7
// A hovered label is drawn at least this big on screen.
const HOVER_FONT = 12

function showLabels(nodes: NodeCollection): void {
  if (!cy) return
  const zoom = cy.zoom()
  nodes.not('.rowlabel').forEach((node) => {
    const base = node.hasClass('center') ? 13 : 11
    if (base * zoom >= HOVER_FONT) return
    node.style({ 'font-size': HOVER_FONT / zoom, 'text-background-color': resolvedColor('--ui-bg'), 'text-background-opacity': 0.85, 'text-background-padding': 2 / zoom, 'z-index': 20 })
    node.addClass('labelled')
  })
}
function hideLabels(): void {
  cy?.$('node.labelled').removeStyle('font-size text-background-color text-background-opacity text-background-padding z-index').removeClass('labelled')
}

// Left before Cytoscape loaded: the graph is not made at all (nothing would destroy it).
let gone = false
onMounted(async () => {
  // With the layered layout (dagre) registered once for the app (app/utils/cardLayout.ts).
  const cytoscape = await cytoscapeWithDagre()
  if (gone) return
  cy = cytoscape({ container: container.value, style: props.stylesheet(), minZoom: 0.05, maxZoom: 3 })
  cy.on('tap', 'node', event => emit('node', event.target.id()))
  cy.on('dbltap', 'node', event => emit('open', event.target.id()))
  cy.on('tap', 'edge', event => emit('edge', event.target.id()))
  cy.on('tap', (event) => {
    if (event.target === cy) emit('background')
  })
  cy.on('mouseover', 'node', (event) => {
    cy!.elements().addClass('faded')
    const near = event.target.closedNeighborhood()
    near.removeClass('faded')
    showLabels(event.target)
  })
  cy.on('mouseout', 'node', () => {
    cy!.elements().removeClass('faded')
    hideLabels()
  })
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
