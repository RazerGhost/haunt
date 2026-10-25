import type { StylesheetJson } from 'cytoscape'

const CHAR = 6.7
const LINE = 14

/**
 * A node's size for its label, worked out here: sizing by the label (`width: 'label'`) waits for the renderer to measure
 * the text, and until it has, the node counts as hidden and so do its edges (in a background tab, for good).
 */
export function nodeSize(label: string, fontSize = 11): { w: number, h: number } {
  const lines = label.split('\n')
  const scale = fontSize / 11
  return { w: Math.ceil(Math.max(...lines.map(line => line.length)) * CHAR * scale + 20), h: Math.ceil(lines.length * LINE * scale + 14) }
}

/** The stylesheet both graphs share, in the current theme's colours. */
export function graphStyle(): StylesheetJson {
  const text = resolvedColor('--ui-text-highlighted')
  const muted = resolvedColor('--ui-text-muted')
  const background = resolvedColor('--ui-bg')
  const warning = resolvedColor('--ui-warning')
  return [
    {
      selector: 'node',
      style: {
        'shape': 'round-rectangle',
        'width': 'data(w)',
        'height': 'data(h)',
        'background-color': background,
        'border-width': 2,
        'border-color': 'data(color)',
        'label': 'data(label)',
        'text-wrap': 'wrap',
        'text-valign': 'center',
        'text-halign': 'center',
        'font-family': 'ui-monospace, Cascadia Mono, Menlo, Consolas, monospace',
        'font-size': 11,
        'color': text
      }
    },
    { selector: 'node.center', style: { 'border-width': 4, 'font-weight': 'bold', 'font-size': 13 } },
    { selector: 'node.more', style: { 'border-style': 'dashed', 'color': muted } },
    // A group of functions in the call graph (one folder's), opened by a click.
    { selector: 'node.group', style: { 'border-style': 'double', 'border-width': 5 } },
    // Against the Changes view's base: changed functions glow amber, the ones that call them get a dashed amber outline.
    { selector: 'node.changed', style: { 'underlay-color': warning, 'underlay-opacity': 0.35, 'underlay-padding': 6, 'underlay-shape': 'round-rectangle' } },
    { selector: 'node.affected', style: { 'outline-width': 2, 'outline-color': warning, 'outline-style': 'dashed', 'outline-offset': 3 } },
    // On the Calls page's path view: an entry point (nothing calls it), and the label of a row of steps.
    { selector: 'node.entry', style: { 'border-style': 'double', 'border-width': 4 } },
    { selector: 'node.rowlabel', style: { 'border-width': 0, 'background-opacity': 0, 'color': muted, 'font-family': 'system-ui, sans-serif', 'font-size': 11, 'text-halign': 'right', 'events': 'no' } },
    { selector: 'node.selected', style: { 'underlay-color': text, 'underlay-opacity': 0.18, 'underlay-padding': 7, 'underlay-shape': 'round-rectangle' } },
    {
      selector: 'edge',
      style: {
        'width': 'data(width)',
        'line-color': 'data(color)',
        'target-arrow-color': 'data(color)',
        'target-arrow-shape': 'triangle',
        'arrow-scale': 0.8,
        'curve-style': 'bezier',
        'opacity': 0.75
      }
    },
    { selector: 'edge.dashed', style: { 'line-style': 'dashed' } },
    // On the import map, imports that keep the rules step back so the ones that break them stand out.
    { selector: 'edge.ok', style: { opacity: 0.25 } },
    { selector: 'edge.known, edge.new', style: { 'opacity': 1, 'z-index': 10 } },
    { selector: 'edge.selected', style: { 'opacity': 1, 'underlay-color': text, 'underlay-opacity': 0.15, 'underlay-padding': 4 } },
    { selector: '.faded', style: { opacity: 0.12 } }
  ]
}
