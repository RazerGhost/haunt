import type cytoscape from 'cytoscape'
import type { CodeGraph, GraphCard } from '~~/shared/codeGraph'
import { columnLayout } from '~~/shared/columnLayout'
import type { LayoutAnswer, LayoutAsk } from '~/workers/cardLayout.worker'

/*
 * Where the code graph's cards go (CodeGraph.vue draws them). Cards with columns (the project's layers, a function's
 * callers and callees) go in those columns (shared/columnLayout.ts). The rest (a part, Predict): a layered layout, left
 * to right, from dagre, each card as big as its rows, and cards with no edge at all packed in a grid under the rest.
 * dagre runs through Cytoscape without drawing anything (headless), so it costs no library of its own, and in a worker,
 * so a big graph does not hold up the window; the last few layouts are kept.
 */

/** The sizes the card is drawn with, in CSS pixels at zoom 1 (CodeGraph.vue uses the same). */
export const CARD = { width: 300, more: 220, header: 36, row: 26, fold: 26, bottom: 6, slotTop: 4 } as const

interface Size { w: number, h: number }
export interface Placed extends Size { x: number, y: number }

/** Where a card's first row starts. */
export function rowsTop(card: GraphCard): number {
  return card.kind === 'slot' ? CARD.slotTop : CARD.header
}

export function cardSize(card: GraphCard): Size {
  if (card.kind === 'more') return { w: CARD.more, h: 52 }
  if (card.kind === 'slot') return { w: 220, h: CARD.slotTop * 2 + card.rows.length * CARD.row }
  if (card.shut) return { w: CARD.width, h: CARD.header + 8 }
  return { w: CARD.width, h: CARD.header + card.rows.length * CARD.row + (card.folded || card.open ? CARD.fold : 0) + CARD.bottom }
}

let loading: Promise<typeof cytoscape> | null = null
/** Cytoscape with the dagre layout registered, loaded once for the app (GraphView uses it too). */
export function cytoscapeWithDagre(): Promise<typeof cytoscape> {
  loading ??= Promise.all([import('cytoscape').then(module => module.default), import('cytoscape-dagre').then(module => module.default)]).then(([core, dagre]) => {
    core.use(dagre)
    return core
  })
  return loading
}

const GAP = 28
const RANK_GAP = 120

/* dagre in a worker (app/workers/cardLayout.worker.ts), so a big graph does not freeze the window; on the window's
 * thread only if a worker cannot start. */
let worker: Worker | null | undefined
let asks = 0
const waiting = new Map<number, (centres: [string, number, number][]) => void>()
function layoutWorker(): Worker | null {
  if (worker !== undefined) return worker
  try {
    worker = new Worker(new URL('../workers/cardLayout.worker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (event: MessageEvent<LayoutAnswer>) => {
      waiting.get(event.data.ask)?.(event.data.centres)
      waiting.delete(event.data.ask)
    }
    worker.onerror = () => {
      // Broken: everything waiting is laid out here instead, and so is what comes after.
      worker = null
      for (const [ask, done] of waiting) failed.get(ask)?.(done)
      waiting.clear()
    }
  } catch {
    worker = null
  }
  return worker
}
const failed = new Map<number, (done: (centres: [string, number, number][]) => void) => void>()

async function dagreHere(nodes: LayoutAsk['nodes'], links: LayoutAsk['links'], rankDir: 'LR' | 'TB'): Promise<[string, number, number][]> {
  const core = await cytoscapeWithDagre()
  const cy = core({
    headless: true,
    styleEnabled: true,
    style: [{ selector: 'node', style: { width: 'data(w)', height: 'data(h)', shape: 'rectangle' } }],
    elements: [...nodes.map(data => ({ data })), ...links.map(data => ({ data }))]
  })
  cy.layout({ name: 'dagre', rankDir, nodeSep: GAP, rankSep: RANK_GAP, edgeSep: 10 } as cytoscape.LayoutOptions).run()
  const centres: [string, number, number][] = cy.nodes().map((node) => {
    const { x, y } = node.position()
    return [node.id(), x, y]
  })
  cy.destroy()
  return centres
}

function dagre(nodes: LayoutAsk['nodes'], links: LayoutAsk['links'], rankDir: 'LR' | 'TB'): Promise<[string, number, number][]> {
  const at = layoutWorker()
  if (!at) return dagreHere(nodes, links, rankDir)
  const ask = ++asks
  return new Promise((resolve) => {
    waiting.set(ask, resolve)
    failed.set(ask, done => void dagreHere(nodes, links, rankDir).then(done))
    at.postMessage({ ask, nodes, links, rankDir, nodeSep: GAP, rankSep: RANK_GAP } satisfies LayoutAsk)
  }).finally(() => failed.delete(ask)) as Promise<[string, number, number][]>
}

/** The last few layouts, by what went in: going back to a graph places it at once. */
const kept = new Map<string, Map<string, Placed>>()
const KEEP = 12

/** The cards placed: the top-left corner and size of each, by card id. */
export async function layoutCards(graph: CodeGraph, rankDir: 'LR' | 'TB' = 'LR'): Promise<Map<string, Placed>> {
  const placed = new Map<string, Placed>()
  if (!graph.cards.length) return placed
  const cardOf = new Map<string, string>()
  for (const card of graph.cards) {
    cardOf.set(card.id, card.id)
    for (const row of card.rows) cardOf.set(row.id, card.id)
  }
  const links = new Map<string, { source: string, target: string }>()
  for (const edge of graph.edges) {
    const source = cardOf.get(edge.from)
    const target = cardOf.get(edge.to)
    if (source && target && source !== target) links.set(`${source}>${target}`, { source, target })
  }
  const joined = new Set([...links.values()].flatMap(link => [link.source, link.target]))
  const sizes = new Map(graph.cards.map(card => [card.id, cardSize(card)]))

  // Cards with columns that mean something (layers, steps from a function): placed in them, here and at once.
  if (graph.cards.some(card => card.column !== undefined)) {
    const last = Math.max(...graph.cards.map(card => card.column ?? -Infinity))
    const nodes = graph.cards.map(card => ({ id: card.id, column: card.column ?? last + 1, ...sizes.get(card.id)! }))
    const key = JSON.stringify(['columns', nodes, [...links.keys()]])
    const known = kept.get(key)
    if (known) return known
    const placed = columnLayout(nodes, [...links.values()], { gap: GAP, columnGap: RANK_GAP + 20, maxHeight: 1600 })
    kept.set(key, placed)
    if (kept.size > KEEP) kept.delete(kept.keys().next().value!)
    return placed
  }

  const nodes = graph.cards.filter(card => joined.has(card.id)).map(card => ({ id: card.id, ...sizes.get(card.id)! }))
  const edges = [...links].map(([id, link]) => ({ id, ...link }))
  const key = JSON.stringify([rankDir, nodes, edges, graph.cards.filter(card => !joined.has(card.id)).map(card => [card.id, sizes.get(card.id)])])
  const known = kept.get(key)
  if (known) return known

  let bottom = 0
  let left = 0
  if (joined.size) {
    for (const [id, x, y] of await dagre(nodes, edges, rankDir)) {
      const { w, h } = sizes.get(id)!
      placed.set(id, { x: x - w / 2, y: y - h / 2, w, h })
    }
    left = Math.min(...[...placed.values()].map(place => place.x))
    bottom = Math.max(...[...placed.values()].map(place => place.y + place.h)) + GAP * 2
  }

  // The rest in rows under it, as wide as the graph above (or a square-ish grid).
  const rest = graph.cards.filter(card => !joined.has(card.id))
  if (rest.length) {
    const right = placed.size ? Math.max(...[...placed.values()].map(place => place.x + place.w)) : 0
    const columns = Math.max(Math.ceil(Math.sqrt(rest.length)), Math.floor((right - left) / (CARD.width + GAP)))
    let x = left
    let y = bottom
    let rowHeight = 0
    rest.forEach((card, at) => {
      if (at && at % columns === 0) {
        x = left
        y += rowHeight + GAP
        rowHeight = 0
      }
      const { w, h } = sizes.get(card.id)!
      placed.set(card.id, { x, y, w, h })
      x += w + GAP
      rowHeight = Math.max(rowHeight, h)
    })
  }
  kept.set(key, placed)
  if (kept.size > KEEP) kept.delete(kept.keys().next().value!)
  return placed
}
