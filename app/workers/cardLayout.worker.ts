/*
 * The code graph's layout (app/utils/cardLayout.ts) off the window's thread: dagre through a headless Cytoscape takes
 * up to a few hundred milliseconds on a big part (Shade's kernel, its project view with 200 import lines), long enough
 * to freeze the window if it ran there.
 */
import cytoscape from 'cytoscape'
import dagre from 'cytoscape-dagre'

export interface LayoutAsk {
  ask: number
  nodes: { id: string, w: number, h: number }[]
  links: { id: string, source: string, target: string }[]
  rankDir: 'LR' | 'TB'
  nodeSep: number
  rankSep: number
}
export interface LayoutAnswer { ask: number, centres: [string, number, number][] }

cytoscape.use(dagre)

self.onmessage = (event: MessageEvent<LayoutAsk>) => {
  const { ask, nodes, links, rankDir, nodeSep, rankSep } = event.data
  const cy = cytoscape({
    headless: true,
    styleEnabled: true,
    style: [{ selector: 'node', style: { width: 'data(w)', height: 'data(h)', shape: 'rectangle' } }],
    elements: [...nodes.map(data => ({ data })), ...links.map(data => ({ data }))]
  })
  cy.layout({ name: 'dagre', rankDir, nodeSep, rankSep, edgeSep: 10 } as cytoscape.LayoutOptions).run()
  const centres: [string, number, number][] = cy.nodes().map((node) => {
    const { x, y } = node.position()
    return [node.id(), x, y]
  })
  cy.destroy()
  self.postMessage({ ask, centres } satisfies LayoutAnswer)
}
