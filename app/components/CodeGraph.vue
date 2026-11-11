<script setup lang="ts">
import type { CodeGraph, GraphCard, GraphEdge, GraphRow } from '~~/shared/codeGraph'
import type { SvgGraphCard, SvgGraphLine } from '~~/shared/exportSvg'
import type { FunctionNode } from '~~/shared/types'
import { CARD, rowsTop, type Placed } from '~/utils/cardLayout'

/*
 * The code graph drawn: each card a file (or a part, in the project view) with its functions (or files) as rows, and
 * the calls (or imports) as lines from row to row, over one layer that pans and zooms. The page works out what to show
 * (shared/codeGraph.ts) and where (app/utils/cardLayout.ts); this draws it and reports clicks.
 *
 * Hover a row or a card to bring its lines forward; the rest step back. Calls within one card are a mark on the row,
 * drawn as lines only on hover. Zoomed far out, rows are drawn as bars.
 *
 * Kept fast for big graphs (a part with hundreds of imports): pan and zoom move one layer without Vue drawing anything
 * again, the lines' shapes are worked out once per layout, a hover draws only its own lines on top (the rest dim with
 * one class), and a card is drawn again only when something it shows changed (v-memo).
 */
export interface SlotLook { label: string, tone: 'empty' | 'picked' | 'active' | 'right' | 'wrong' | 'missed' }

const props = defineProps<{
  graph: CodeGraph
  places: Map<string, Placed>
  /** The function (row) or card the graph is about. */
  center?: string | null
  selected?: string | null
  /** Rows whose name the search finds. */
  term?: string
  entries?: ReadonlySet<string>
  familiar?: Map<string, 'familiar' | 'stale'> | null
  /** Per file, how busy it was, 0–1. */
  heat?: Map<string, number> | null
  slot?: (id: string) => SlotLook
}>()
const emit = defineEmits<{
  row: [id: string]
  open: [id: string]
  header: [card: GraphCard]
  fold: [card: GraphCard]
  shut: [card: GraphCard]
  background: []
}>()

const { index, scan } = useCode()
const { changedById, affected } = useChanges()

/* Pan and zoom: one transform on the layer, written straight to it. */
const viewport = useTemplateRef<HTMLElement>('viewport')
const layer = useTemplateRef<HTMLElement>('layer')
const view = { x: 0, y: 0, k: 1 }
const size = { w: 0, h: 0 }
const MIN = 0.08
const MAX = 2
const FAR = 0.45
/** Zoomed out so far that names cannot be read: rows are bars. Changes only when the zoom crosses the line. */
const far = ref(false)
let frame = 0
/*
 * The layer is its own GPU layer (will-change) only while it moves, and for a moment after. Kept that way, Chrome would
 * raster it at the scale it first had: a big graph fitted to the window is thousands of pixels across, drawn whole at
 * full size, for frames on end (measured on Shade's kernel: six frames of 80–380 ms with the page itself idle). Left
 * alone, only what shows is drawn, at the zoom it shows at.
 */
let settle = 0
function moving(): void {
  if (layer.value && !settle) layer.value.style.willChange = 'transform'
  clearTimeout(settle)
  settle = window.setTimeout(() => {
    settle = 0
    if (layer.value) layer.value.style.willChange = ''
  }, 250)
}
function apply(): void {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    if (layer.value) layer.value.style.transform = `translate(${view.x}px, ${view.y}px) scale(${view.k})`
    if (far.value !== view.k < FAR) far.value = view.k < FAR
    drawInk()
  })
}

const bounds = computed(() => {
  const all = [...props.places.values()]
  if (!all.length) return { x: 0, y: 0, w: 0, h: 0 }
  const x = Math.min(...all.map(place => place.x))
  const y = Math.min(...all.map(place => place.y))
  return { x, y, w: Math.max(...all.map(place => place.x + place.w)) - x, h: Math.max(...all.map(place => place.y + place.h)) - y }
})

/** Everything in view, no bigger than life. */
function fit(): void {
  const box = bounds.value
  if (!size.w || !box.w) return
  const k = Math.max(MIN, Math.min(1, (size.w - 60) / box.w, (size.h - 60) / box.h))
  view.k = k
  view.x = (size.w - box.w * k) / 2 - box.x * k
  view.y = (size.h - box.h * k) / 2 - box.y * k
  apply()
}
/** Opens on everything when it can be read whole; else on the centre at a readable size. */
function home(): void {
  fit()
  if (view.k >= 0.55 || !props.center) return
  const card = cardOf.value.get(props.center)
  const place = card ? props.places.get(card.id) : undefined
  const at = props.center ? pointOf(props.center, 'in') : null
  if (!place || !at) return
  view.k = 0.85
  view.x = size.w / 2 - (place.x + place.w / 2) * view.k
  view.y = size.h / 2 - at.y * view.k
  apply()
}

// A new set of cards opens fitted; the same cards again (a row opened, a card shut) keep your pan and zoom.
let shown = ''
watch(() => props.places, () => {
  const key = props.graph.cards.map(card => card.id).join('\n')
  if (key !== shown) {
    shown = key
    nextTick(home)
    mountInTurns()
  }
})

/* A big graph's cards are put on the page a few per frame (the focus's first), so the window answers while they come. */
const BATCH = 12
const mounted = ref(0)
let mounting = 0
function mountInTurns(): void {
  cancelAnimationFrame(mounting)
  mounted.value = Math.min(BATCH, props.graph.cards.length)
  const more = () => {
    if (mounted.value >= props.graph.cards.length) return
    mounted.value = Math.min(props.graph.cards.length, mounted.value + BATCH)
    mounting = requestAnimationFrame(more)
  }
  mounting = requestAnimationFrame(more)
}
const cardsShown = computed(() => (mounted.value >= props.graph.cards.length ? props.graph.cards : props.graph.cards.slice(0, mounted.value)))
// The same cards changed (a row opened): all of them at once.
watch(() => props.graph, (now, before) => {
  if (before && now.cards.length !== before.cards.length && shown === now.cards.map(card => card.id).join('\n')) mounted.value = now.cards.length
})

function zoomAt(px: number, py: number, k: number): void {
  const next = Math.max(MIN, Math.min(MAX, k))
  view.x = px - (px - view.x) * (next / view.k)
  view.y = py - (py - view.y) * (next / view.k)
  view.k = next
  apply()
}
function wheel(event: WheelEvent): void {
  moving()
  const box = viewport.value!.getBoundingClientRect()
  // A touchpad pans with two fingers and zooms with a pinch (ctrlKey); a mouse wheel zooms.
  if (!event.ctrlKey && Math.abs(event.deltaX) > 0 && event.deltaMode === 0 && Math.abs(event.deltaY) < 50) {
    view.x -= event.deltaX
    view.y -= event.deltaY
    apply()
    return
  }
  zoomAt(event.clientX - box.left, event.clientY - box.top, view.k * Math.exp(-event.deltaY * (event.ctrlKey ? 0.01 : 0.0015)))
}
function zoomBy(factor: number): void {
  zoomAt(size.w / 2, size.h / 2, view.k * factor)
}

let drag: { x: number, y: number, vx: number, vy: number, moved: boolean, id: number } | null = null
let dragged = false
const panning = ref(false)
function down(event: PointerEvent): void {
  if (event.button !== 0 && event.button !== 1) return
  drag = { x: event.clientX, y: event.clientY, vx: view.x, vy: view.y, moved: false, id: event.pointerId }
}
function move(event: PointerEvent): void {
  if (!drag || drag.id !== event.pointerId) return
  const dx = event.clientX - drag.x
  const dy = event.clientY - drag.y
  if (!drag.moved && Math.hypot(dx, dy) < 4) return
  if (!drag.moved) {
    drag.moved = true
    panning.value = true
    hovered.value = null
    viewport.value?.setPointerCapture(event.pointerId)
  }
  moving()
  view.x = drag.vx + dx
  view.y = drag.vy + dy
  apply()
}
function up(event: PointerEvent): void {
  if (!drag || drag.id !== event.pointerId) return
  dragged = drag.moved
  drag = null
  panning.value = false
}
/** A drag ends with a click on what it started on: swallowed. */
function swallow(event: MouseEvent): void {
  if (!dragged) return
  dragged = false
  event.stopPropagation()
  event.preventDefault()
}
function background(event: MouseEvent): void {
  const target = event.target as Element
  if (target === event.currentTarget || target === layer.value) emit('background')
}

let observer: ResizeObserver | null = null
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    const first = !size.w
    size.w = entry!.contentRect.width
    size.h = entry!.contentRect.height
    if (first) home()
    else apply()
  })
  observer.observe(viewport.value!)
})
onBeforeUnmount(() => {
  observer?.disconnect()
  cancelAnimationFrame(frame)
  cancelAnimationFrame(mounting)
  clearTimeout(settle)
})
defineExpose({ fit, zoomBy, picture })

/* What the rows and cards say. */
const cardOf = computed(() => {
  const out = new Map<string, GraphCard>()
  for (const card of props.graph.cards) {
    out.set(card.id, card)
    for (const row of card.rows) out.set(row.id, card)
  }
  return out
})
const rowAt = computed(() => {
  const out = new Map<string, number>()
  for (const card of props.graph.cards) card.rows.forEach((row, at) => out.set(row.id, at))
  return out
})
const fnOf = (row: GraphRow): FunctionNode | null => (row.kind === 'function' ? index.value?.functions.get(row.id)?.fn ?? null : null)
const KIND = { function: 'f', arrow: 'a', method: 'm', class: 'c' } as const
const KIND_COLOR = {
  function: 'text-sky-600 dark:text-sky-400 bg-sky-500/10',
  arrow: 'text-amber-600 dark:text-amber-400 bg-amber-500/10',
  method: 'text-emerald-600 dark:text-emerald-400 bg-emerald-500/10',
  class: 'text-rose-600 dark:text-rose-400 bg-rose-500/10'
} as const
const baseOf = (path: string) => path.split('/').pop() ?? path
const nameOf = (row: GraphRow) => {
  if (row.kind === 'file') return baseOf(row.id)
  const fn = fnOf(row)
  return fn ? (fn.anonymous ? t('CodeGraph.callback') : fn.name) : row.id
}
const term = computed(() => props.term?.trim().toLowerCase() ?? '')
const found = (row: GraphRow) => !!term.value && !row.context && nameOf(row).toLowerCase().includes(term.value)

const parts = computed(() => new Map((scan.value?.parts ?? []).map(part => [part.name, part])))
const files = computed(() => new Map((scan.value?.files ?? []).map(file => [file.path, file])))
const partOf = (card: GraphCard) => (card.kind === 'file' ? files.value.get(card.name)?.part ?? '' : card.name)
const layerColorOf = (card: GraphCard) => layerColor(parts.value.get(partOf(card))?.layer ?? 'unmapped')
/** A file's folder, without its part's own folder (every card of a part shares it), for after the name. */
function folderOf(path: string): string {
  const part = parts.value.get(files.value.get(path)?.part ?? '')
  const inside = part?.folder && path.startsWith(`${part.folder}/`) ? path.slice(part.folder.length + 1) : path
  return inside.includes('/') ? inside.slice(0, inside.lastIndexOf('/')) : ''
}

/** The share of a file's functions you know, while Familiarity is on. */
function knownShare(card: GraphCard): number | null {
  if (!props.familiar || card.kind !== 'file') return null
  const file = files.value.get(card.name)
  if (!file) return null
  let all = 0
  let known = 0
  const walk = (items: FunctionNode[]) => items.forEach((fn) => {
    if (!fn.anonymous && !fn.test) {
      all++
      if (props.familiar!.get(fn.id) === 'familiar') known++
    }
    walk(fn.children)
  })
  walk(file.functions)
  return all ? known / all : null
}
function headerTint(card: GraphCard): Record<string, string> {
  const heat = card.kind === 'file' ? props.heat?.get(card.name) ?? 0 : 0
  return heat > 0.05 ? { background: `color-mix(in oklab, var(--ui-error) ${Math.round(heat * 28)}%, var(--ui-bg-elevated))` } : {}
}
function headerTitle(card: GraphCard): string {
  if (card.kind === 'file') return t('CodeGraph.focusFile', { path: card.name })
  if (card.kind === 'idle') return t('CodeGraph.idleHelp')
  return t('CodeGraph.focusPart', { name: card.name })
}

/** Calls within one card, per row: shown as a mark. */
const innerCount = computed(() => {
  const out = new Map<string, number>()
  for (const edge of props.graph.inner) {
    out.set(edge.from, (out.get(edge.from) ?? 0) + 1)
    out.set(edge.to, (out.get(edge.to) ?? 0) + 1)
  }
  return out
})

/* The lines: their shapes once per layout, their light per hover. */
function pointOf(anchor: string, side: 'in' | 'out') {
  const card = cardOf.value.get(anchor)
  const place = card ? props.places.get(card.id) : undefined
  if (!card || !place) return null
  const at = rowAt.value.get(anchor)
  const y = anchor === card.id || card.shut || at === undefined ? place.y + Math.min(CARD.header, place.h) / 2 : place.y + rowsTop(card) + at * CARD.row + CARD.row / 2
  return { x: side === 'in' ? place.x : place.x + place.w, y, place }
}
/** A line as one cubic curve: from, two control points, to. */
type Curve = [number, number, number, number, number, number, number, number]
function shapeOf(edge: GraphEdge, inner: boolean): Curve | null {
  const a = pointOf(edge.from, 'out')
  const b = pointOf(edge.to, 'in')
  if (!a || !b) return null
  if (inner) {
    // Within one card: a small loop out of its left side.
    const x = a.place.x
    const reach = 18 + Math.min(40, Math.abs(b.y - a.y) / 5)
    return [x, a.y, x - reach, a.y, x - reach, b.y, x, b.y]
  }
  if (b.x >= a.x + 8) {
    // Forward: out of the right side of one into the left side of the other.
    const bend = Math.max(36, (b.x - a.x) / 2)
    return [a.x, a.y, a.x + bend, a.y, b.x - bend, b.y, b.x, b.y]
  }
  if (b.place.x + b.place.w <= a.place.x - 8) {
    // Backward: out of the left side, into the right side.
    const sx = a.place.x
    const tx = b.place.x + b.place.w
    const bend = Math.max(36, (sx - tx) / 2)
    return [sx, a.y, sx - bend, a.y, tx + bend, b.y, tx, b.y]
  }
  // In the same column: a loop out to the right and back.
  const sx = a.place.x + a.place.w
  const tx = b.place.x + b.place.w
  const reach = 30 + Math.min(80, Math.abs(b.y - a.y) / 4)
  return [sx, a.y, sx + reach, a.y, tx + reach, b.y, tx, b.y]
}
type Tone = 'call' | 'import' | 'new' | 'known' | 'ok'
interface Line { id: string, curve: Curve, box: [number, number, number, number], edge: GraphEdge, tone: Tone, width: number }
const toneOf = (edge: GraphEdge): Tone => (edge.kind === 'call' ? 'call' : edge.tone === 'new' ? 'new' : edge.tone === 'known' ? 'known' : edge.tone === 'ok' ? 'ok' : 'import')
const COLOR_VARS: Record<Tone | 'lit', string> = { call: '--ui-text-dimmed', import: '--ui-info', ok: '--ui-text-dimmed', new: '--rule-new', known: '--rule-known', lit: '--ui-primary' }
const widthOf = (edge: GraphEdge) => Math.min(4, 1.2 + Math.log2(edge.count) * 0.8)
function linesOf(edges: GraphEdge[], inner: boolean): Line[] {
  const out: Line[] = []
  for (const edge of edges) {
    const curve = shapeOf(edge, inner)
    if (!curve) continue
    const xs = [curve[0], curve[2], curve[4], curve[6]]
    const ys = [curve[1], curve[3], curve[5], curve[7]]
    out.push({ id: edge.id, curve, box: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)], edge, tone: toneOf(edge), width: widthOf(edge) })
  }
  return out
}
const lines = computed(() => linesOf(props.graph.edges, false))
const innerLines = computed(() => linesOf(props.graph.inner, true))
/** The lines by the ids they end on, to find a row's or a card's quickly. */
const linesAt = computed(() => {
  const out = new Map<string, Line[]>()
  for (const line of [...lines.value, ...innerLines.value]) {
    for (const end of [line.edge.from, line.edge.to]) {
      const list = out.get(end)
      if (list) list.push(line)
      else out.set(end, [line])
    }
  }
  return out
})

/*
 * Many lines (a part's hundreds of imports, a much-called function's callers): the ones between the same two cards
 * are drawn as one, as thick as their count, from card to card. Hovering a row still draws its own lines, row to row.
 * Fewer lines to read, and fewer to draw: zoomed in, each one is long and wide, and panning drew them all every frame.
 */
const BUNDLE_OVER = 150
const TONE_ORDER = ['plain', 'ok', 'known', 'new'] as const
const drawn = computed<Line[]>(() => {
  if (lines.value.length <= BUNDLE_OVER) return lines.value
  const groups = new Map<string, { from: string, to: string, list: Line[] }>()
  for (const line of lines.value) {
    const from = cardOf.value.get(line.edge.from)?.id ?? line.edge.from
    const to = cardOf.value.get(line.edge.to)?.id ?? line.edge.to
    const key = `${from}\n${to}\n${line.edge.kind}`
    const group = groups.get(key)
    if (group) group.list.push(line)
    else groups.set(key, { from, to, list: [line] })
  }
  const out: Line[] = []
  for (const [key, { from, to, list }] of groups) {
    const first = list[0]!
    if (list.length === 1) {
      out.push(first)
      continue
    }
    const edge: GraphEdge = {
      id: `bundle:${key}`,
      // One row on either end stays that row; several become the card.
      from: list.every(line => line.edge.from === first.edge.from) ? first.edge.from : from,
      to: list.every(line => line.edge.to === first.edge.to) ? first.edge.to : to,
      kind: first.edge.kind,
      count: list.reduce((sum, line) => sum + line.edge.count, 0),
      guessed: list.every(line => line.edge.guessed),
      typeOnly: list.every(line => line.edge.typeOnly),
      tone: list.reduce((worst, line) => (TONE_ORDER.indexOf(line.edge.tone) > TONE_ORDER.indexOf(worst) ? line.edge.tone : worst), first.edge.tone)
    }
    out.push(...linesOf([edge], false))
  }
  return out
})
/** Dashes only while few lines are drawn: on long lines zoomed in, they are thousands of strokes each. */
const DASHES_UP_TO = 200

const hovered = ref<string | null>(null)
/** The lines a hover or the selection lights up, drawn again on top: a row's own lines; a card's drawn ones. */
function linesFor(id: string | null | undefined): Line[] {
  if (!id) return []
  const card = props.graph.cards.find(item => item.id === id)
  if (!card) return linesAt.value.get(id) ?? []
  if (drawn.value !== lines.value) return drawn.value.filter(line => cardOf.value.get(line.edge.from) === card || cardOf.value.get(line.edge.to) === card)
  const ids = [card.id, ...card.rows.map(row => row.id)]
  return [...new Set(ids.flatMap(item => linesAt.value.get(item) ?? []))]
}
const litLines = computed(() => [...new Set([...linesFor(hovered.value), ...linesFor(props.selected)])])
const dim = computed(() => !!hovered.value)
/** The cards a hover reaches: the hovered one and the ones at the other end of its lines. */
const litCards = computed(() => {
  const out = new Set<string>()
  if (!hovered.value) return out
  const own = cardOf.value.get(hovered.value)
  if (own) out.add(own.id)
  for (const line of linesFor(hovered.value)) {
    for (const end of [line.edge.from, line.edge.to]) {
      const card = cardOf.value.get(end)
      if (card) out.add(card.id)
    }
  }
  return out
})

/*
 * The lines on a canvas the size of the window, drawn again on every pan and zoom: a few milliseconds for hundreds of
 * curves. As SVG they cost far more: the window is rastered in tiles, and every tile a long curve crosses drew it again
 * (Shade's kernel, 537 lines: nine frames of 80–150 ms with the page idle).
 */
const ink = useTemplateRef<HTMLCanvasElement>('ink')
const colorMode = useColorMode()
const palette = shallowRef<Record<Tone | 'lit', string> | null>(null)
const readPalette = () => {
  palette.value = Object.fromEntries(Object.entries(COLOR_VARS).map(([tone, name]) => [tone, resolvedColor(name)])) as Record<Tone | 'lit', string>
}
watch(() => colorMode.value, () => nextTick(() => {
  readPalette()
  apply()
}))
const ARROW = 7
function drawInk(): void {
  const canvas = ink.value
  if (!canvas || !size.w) return
  const ratio = window.devicePixelRatio || 1
  const w = Math.round(size.w * ratio)
  const h = Math.round(size.h * ratio)
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w
    canvas.height = h
  }
  const context = canvas.getContext('2d')
  if (!context) return
  context.setTransform(1, 0, 0, 1, 0, 0)
  context.clearRect(0, 0, w, h)
  if (!palette.value) readPalette()
  const colors = palette.value!
  context.setTransform(ratio * view.k, 0, 0, ratio * view.k, ratio * view.x, ratio * view.y)
  // Only the lines whose box is in view.
  const left = -view.x / view.k - 20
  const top = -view.y / view.k - 20
  const right = left + size.w / view.k + 40
  const bottom = top + size.h / view.k + 40
  const inView = (line: Line) => line.box[2] >= left && line.box[0] <= right && line.box[3] >= top && line.box[1] <= bottom
  const dashes = drawn.value.length <= DASHES_UP_TO
  const stroke = (line: Line, color: string, alpha: number, wider: number, lit = false) => {
    const [x0, y0, ax, ay, bx, by, x1, y1] = line.curve
    context.globalAlpha = alpha
    context.strokeStyle = color
    context.fillStyle = color
    context.lineWidth = line.width + wider
    const loose = line.edge.guessed || line.edge.typeOnly
    context.setLineDash(loose && (dashes || lit) ? [5, 4] : [])
    if (loose && !dashes && !lit) context.globalAlpha = alpha * 0.55
    context.beginPath()
    context.moveTo(x0, y0)
    context.bezierCurveTo(ax, ay, bx, by, x1, y1)
    context.stroke()
    // The arrowhead, along the curve's last stretch.
    const angle = Math.atan2(y1 - by, x1 - bx)
    const cos = Math.cos(angle)
    const sin = Math.sin(angle)
    context.setLineDash([])
    context.beginPath()
    context.moveTo(x1, y1)
    context.lineTo(x1 - ARROW * cos + ARROW * 0.45 * sin, y1 - ARROW * sin - ARROW * 0.45 * cos)
    context.lineTo(x1 - ARROW * cos - ARROW * 0.45 * sin, y1 - ARROW * sin + ARROW * 0.45 * cos)
    context.closePath()
    context.fill()
  }
  const base = dim.value ? 0.15 : 0.85
  for (const line of drawn.value) if (inView(line)) stroke(line, colors[line.tone], line.tone === 'ok' ? base * 0.5 : base, 0)
  for (const line of litLines.value) if (inView(line)) stroke(line, colors.lit, 1, 0.8, true)
  context.globalAlpha = 1
}
// New lines, or a hover: drawn again in the next frame.
watch([drawn, litLines, dim], () => apply())

/** The whole graph as Export draws it (shared/exportSvg.ts graphSvg): every card with its texts, and the lines as drawn. */
function picture(): { cards: SvgGraphCard[], lines: SvgGraphLine[] } {
  if (!palette.value) readPalette()
  const colors = palette.value!
  const cards: SvgGraphCard[] = []
  for (const card of props.graph.cards) {
    const place = props.places.get(card.id)
    if (!place) continue
    const rows = card.shut
      ? []
      : card.rows.map((row) => {
          if (row.kind === 'slot') return { label: props.slot?.(row.id).label ?? '', letter: '', folder: '', depth: 0, context: false, notes: [] }
          const look = lookOf(row)
          const notes = [
            ...props.entries?.has(row.id) ? [t('CodeGraph.entry')] : [],
            ...changedById.value.has(row.id) ? [t('CodeGraph.changed')] : affected.value.has(row.id) ? [t('CodeGraph.affected')] : [],
            ...look.test ? [t('CodeGraph.test')] : [],
            ...innerCount.value.get(row.id) ? [`↻ ${innerCount.value.get(row.id)}`] : [],
            ...look.lines !== null && !row.context ? [String(look.lines)] : []
          ]
          return { label: look.name, letter: look.letter, folder: look.folder, depth: row.depth, context: row.context, notes }
        })
    const shutCount = card.shut && card.rows.length ? (card.rows[0]!.kind === 'file' ? t('common.files', { count: card.rows.length }) : t('common.functions', { count: card.rows.filter(row => !row.context).length })) : ''
    let footer = ''
    if (!card.shut && (card.folded || card.open)) footer = card.kind === 'part' ? t('CodeGraph.partFiles', { count: card.folded }) : card.open ? t('CodeGraph.fewer') : t('CodeGraph.more', { count: card.folded })
    cards.push({
      ...place,
      title: card.kind === 'file' ? baseOf(card.name) : card.kind === 'idle' ? t('CodeGraph.idle', { count: card.rows.length }) : card.kind === 'more' ? t('CodeGraph.moreFiles', { count: card.count ?? 0 }) : card.kind === 'slot' ? '' : card.name,
      folder: card.kind === 'file' ? folderOf(card.name) || shutCount : card.kind === 'more' ? card.name : shutCount,
      layer: card.kind === 'file' || card.kind === 'part' ? parts.value.get(partOf(card))?.layer ?? 'unmapped' : null,
      dashed: card.kind === 'idle' || card.kind === 'slot',
      rowsTop: card.kind === 'more' ? 0 : rowsTop(card),
      row: CARD.row,
      rows,
      footer
    })
  }
  const lines = drawn.value.map(line => ({ curve: line.curve, color: colors[line.tone], width: line.width, dashed: line.edge.guessed || line.edge.typeOnly, opacity: line.tone === 'ok' ? 0.45 : 0.85 }))
  return { cards, lines }
}

/** What a card's drawing depends on, for v-memo: it is drawn again only when one of these changed. */
function memoOf(card: GraphCard): unknown[] {
  // Predict's places change with every pick: always drawn.
  if (card.kind === 'slot') return [{}]
  const mine = (id: string | null | undefined) => (id && cardOf.value.get(id) === card ? id : null)
  return [card, props.places.get(card.id), far.value, mine(props.selected), mine(props.center), props.center === card.id, term.value, props.familiar, props.heat, changedById.value, affected.value, props.entries, dim.value && !litCards.value.has(card.id), innerCount.value, rowLooks.value]
}

/** What each row says, worked out once per graph rather than on every draw. */
interface RowLook { name: string, letter: string, tint: string, folder: string, title: string, lines: number | null, test: boolean }
const rowLooks = computed(() => {
  const out = new Map<string, RowLook>()
  for (const card of props.graph.cards) {
    for (const row of card.rows) {
      if (row.kind === 'slot') continue
      if (row.kind === 'file') {
        out.set(row.id, { name: baseOf(row.id), letter: '', tint: '', folder: folderOf(row.id), title: row.id, lines: null, test: false })
        continue
      }
      const fn = fnOf(row)
      if (!fn) continue
      const callers = index.value?.callers.get(fn.id)?.length ?? 0
      const callees = index.value?.callees.get(fn.id)?.length ?? 0
      out.set(row.id, {
        name: nameOf(row),
        letter: KIND[fn.kind],
        tint: KIND_COLOR[fn.kind],
        folder: '',
        title: `${fn.name} · ${t('CodeGraph.lines', { from: fn.line, to: fn.endLine })} · ${t('CodeGraph.callCounts', { callers, callees })}`,
        lines: fn.endLine - fn.line + 1,
        test: !!fn.test
      })
    }
  }
  return out
})
const NO_LOOK: RowLook = { name: '', letter: '', tint: '', folder: '', title: '', lines: null, test: false }
const lookOf = (row: GraphRow) => rowLooks.value.get(row.id) ?? NO_LOOK
</script>

<template>
  <div
    ref="viewport"
    class="relative size-full touch-none overflow-hidden bg-default select-none"
    :class="panning ? 'cursor-grabbing' : 'cursor-grab'"
    style="background-image: radial-gradient(color-mix(in oklab, var(--ui-text-dimmed) 30%, transparent) 1px, transparent 1px); background-size: 22px 22px"
    @wheel.prevent="wheel"
    @pointerdown="down"
    @pointermove="move"
    @pointerup="up"
    @pointercancel="up"
    @click.capture="swallow"
    @click="background"
  >
    <!-- The lines: drawn on a canvas the size of the window, under the cards. -->
    <canvas ref="ink" class="pointer-events-none absolute inset-0 size-full" aria-hidden="true" />
    <div ref="layer" class="absolute top-0 left-0 origin-top-left">

      <template v-for="card in cardsShown" :key="card.id">
        <div
          v-if="places.get(card.id)"
          v-memo="memoOf(card)"
          class="absolute flex flex-col overflow-hidden rounded-lg border bg-elevated"
          :class="[
            card.home ? 'border-accented' : 'border-default',
            center === card.id ? 'ring-2 ring-primary' : '',
            card.kind === 'slot' ? 'border-dashed' : '',
            card.kind === 'idle' ? 'border-dashed' : '',
            dim && !litCards.has(card.id) ? 'opacity-50' : ''
          ]"
          :style="{ left: `${places.get(card.id)!.x}px`, top: `${places.get(card.id)!.y}px`, width: `${places.get(card.id)!.w}px`, height: `${places.get(card.id)!.h}px` }"
          @pointerenter="!panning && (hovered = card.id)"
          @pointerleave="hovered = null"
        >
          <!-- More files of a part than the graph draws: a click goes to the part. -->
          <button v-if="card.kind === 'more'" type="button" class="grid h-full place-content-center gap-0.5 px-3 text-center hover:bg-default" @click="emit('header', card)">
            <span class="text-ui text-highlighted">{{ t('CodeGraph.moreFiles', { count: card.count ?? 0 }) }}</span>
            <span class="truncate font-mono text-label text-muted">{{ card.name }}</span>
          </button>

          <template v-else>
            <div v-if="card.kind !== 'slot'" class="relative flex shrink-0 items-center gap-1.5 border-b border-default px-2" :style="{ height: `${CARD.header}px`, ...headerTint(card) }">
              <span v-if="card.kind === 'idle'" class="size-2 shrink-0 rounded-full border border-dashed border-(--ui-text-dimmed)" />
              <span v-else class="size-2 shrink-0 rounded-full" :style="{ background: layerColorOf(card) }" />
              <button type="button" class="flex min-w-0 flex-1 items-baseline gap-1.5 text-left" :title="card.kind === 'file' ? card.name : headerTitle(card)" @click="emit('header', card)">
                <template v-if="card.kind === 'file'">
                  <span class="min-w-0 truncate font-mono text-xs font-semibold text-highlighted hover:underline">{{ baseOf(card.name) }}</span>
                  <span class="min-w-0 shrink-[4] truncate font-mono text-label text-muted" dir="rtl">{{ folderOf(card.name) }}</span>
                </template>
                <span v-else-if="card.kind === 'idle'" class="truncate text-xs text-muted">{{ t('CodeGraph.idle', { count: card.rows.length }) }}</span>
                <span v-else class="truncate font-mono text-xs font-semibold text-highlighted hover:underline">{{ card.name }}</span>
              </button>
              <span v-if="card.shut && card.rows.length" class="shrink-0 text-label text-muted tabular-nums">{{ card.rows[0]!.kind === 'file' ? t('common.files', { count: card.rows.length }) : t('common.functions', { count: card.rows.filter(row => !row.context).length }) }}</span>
              <button v-if="card.rows.length" type="button" class="grid size-5 shrink-0 place-items-center rounded text-muted hover:bg-default hover:text-highlighted" :aria-label="card.shut ? t('CodeGraph.unshut') : t('CodeGraph.shut')" :title="card.shut ? t('CodeGraph.unshut') : t('CodeGraph.shut')" @click="emit('shut', card)">
                <span class="text-sm leading-none">{{ card.shut ? '+' : '−' }}</span>
              </button>
              <span v-if="knownShare(card) !== null" class="absolute right-0 bottom-0 left-0 h-0.5 bg-(--ui-text-dimmed)/25">
                <span class="block h-full bg-success" :style="{ width: `${Math.round(knownShare(card)! * 100)}%` }" />
              </span>
            </div>

            <template v-if="!card.shut">
              <div :style="{ paddingTop: card.kind === 'slot' ? `${CARD.slotTop}px` : '0' }">
                <template v-for="row in card.rows" :key="row.id">
                  <!-- A place to fill in (Predict). -->
                  <button
                    v-if="row.kind === 'slot'"
                    type="button"
                    class="mx-1 flex w-[calc(100%-0.5rem)] items-center gap-1.5 rounded border border-dashed px-2 font-mono text-xs"
                    :class="{
                      'border-accented text-muted': slot?.(row.id).tone === 'empty',
                      'border-solid border-accented text-highlighted': slot?.(row.id).tone === 'picked',
                      'border-solid border-primary bg-primary/10 text-highlighted': slot?.(row.id).tone === 'active',
                      'border-solid border-success bg-success/10 text-highlighted': slot?.(row.id).tone === 'right',
                      'border-solid border-error bg-error/10 text-highlighted': slot?.(row.id).tone === 'wrong',
                      'text-muted': slot?.(row.id).tone === 'missed'
                    }"
                    :style="{ height: `${CARD.row - 4}px`, marginBlock: '2px' }"
                    @click="emit('row', row.id)"
                  >
                    <span class="truncate">{{ slot?.(row.id).label }}</span>
                  </button>
                  <button
                    v-else
                    type="button"
                    class="flex w-full items-center gap-1.5 pr-2 text-left"
                    :class="[
                      row.context ? 'cursor-default text-muted' : 'hover:bg-default',
                      selected === row.id ? 'bg-primary/15' : center === row.id ? 'bg-primary/8' : '',
                      found(row) ? 'outline-2 -outline-offset-2 outline-amber-500' : '',
                      changedById.has(row.id) ? 'shadow-[inset_3px_0_0_var(--ui-warning)]' : affected.has(row.id) ? 'shadow-[inset_3px_0_0_color-mix(in_oklab,var(--ui-warning)_45%,transparent)]' : ''
                    ]"
                    :style="{ height: `${CARD.row}px`, paddingLeft: `${8 + row.depth * 14}px` }"
                    :title="lookOf(row).title"
                    @click="!row.context && emit('row', row.id)"
                    @dblclick="!row.context && emit('open', row.id)"
                    @pointerenter="!row.context && !panning && (hovered = row.id)"
                    @pointerleave="hovered = card.id"
                  >
                    <template v-if="far">
                      <span class="h-2 flex-1 rounded-sm" :class="row.context ? 'bg-(--ui-text-dimmed)/20' : 'bg-(--ui-text-dimmed)/45'" />
                    </template>
                    <template v-else>
                      <span v-if="row.depth" class="-ml-1 shrink-0 text-[10px] text-dimmed">↳</span>
                      <span v-if="row.kind === 'file'" class="size-2.5 shrink-0 rounded-[2px] border border-(--ui-text-dimmed)" />
                      <span v-else-if="lookOf(row).letter" class="grid size-4 shrink-0 place-items-center rounded font-mono text-[10px] font-bold" :class="lookOf(row).tint">{{ lookOf(row).letter }}</span>
                      <span class="flex min-w-0 flex-1 items-baseline gap-1.5">
                        <span class="min-w-0 truncate font-mono text-xs" :class="[row.context ? '' : 'text-highlighted', center === row.id ? 'font-semibold' : '']">{{ lookOf(row).name }}</span>
                        <span v-if="lookOf(row).folder" class="min-w-0 shrink-[4] truncate font-mono text-[10px] text-dimmed" dir="rtl">{{ lookOf(row).folder }}</span>
                      </span>
                      <span v-if="entries?.has(row.id)" class="shrink-0 rounded bg-info/15 px-1 text-[10px] text-info">{{ t('CodeGraph.entry') }}</span>
                      <span v-if="changedById.has(row.id)" class="shrink-0 rounded bg-warning/15 px-1 text-[10px] text-warning">{{ t('CodeGraph.changed') }}</span>
                      <span v-else-if="affected.has(row.id)" class="shrink-0 rounded border border-dashed border-warning/60 px-1 text-[10px] text-warning">{{ t('CodeGraph.affected') }}</span>
                      <span v-if="lookOf(row).test" class="shrink-0 rounded bg-(--ui-text-dimmed)/15 px-1 text-[10px] text-muted">{{ t('CodeGraph.test') }}</span>
                      <span v-if="innerCount.get(row.id)" class="shrink-0 text-[10px] text-muted tabular-nums" :title="t('CodeGraph.innerCalls', { count: innerCount.get(row.id)! })">↻ {{ innerCount.get(row.id) }}</span>
                      <span
                        v-if="familiar && row.kind === 'function' && !row.context"
                        class="size-2 shrink-0 rounded-full"
                        :class="familiar.get(row.id) === 'familiar' ? 'bg-success' : familiar.get(row.id) === 'stale' ? 'bg-warning' : 'border border-(--ui-text-dimmed)'"
                        :title="familiar.get(row.id) === 'familiar' ? t('CodeGraph.understood') : familiar.get(row.id) === 'stale' ? t('CodeGraph.changedSince') : t('CodeGraph.notLearned')"
                      />
                      <span v-if="lookOf(row).lines !== null && !row.context" class="w-5 shrink-0 text-right text-[10px] text-dimmed tabular-nums">{{ lookOf(row).lines }}</span>
                    </template>
                  </button>
                </template>
              </div>
              <button
                v-if="card.folded || card.open"
                type="button"
                class="flex w-full shrink-0 items-center gap-1 px-2 text-left text-label text-muted hover:bg-default hover:text-default"
                :style="{ height: `${CARD.fold}px` }"
                @click="emit('fold', card)"
              >
                <template v-if="card.kind === 'part'">{{ t('CodeGraph.partFiles', { count: card.folded }) }}<UIcon name="i-lucide-arrow-right" class="size-3" /></template>
                <template v-else-if="card.open">{{ t('CodeGraph.fewer') }}</template>
                <template v-else>{{ t('CodeGraph.more', { count: card.folded }) }}</template>
              </button>
            </template>
          </template>
        </div>
      </template>
    </div>

    <!-- On the graph, like a map: what the marks mean, and the zoom. -->
    <div class="pointer-events-none absolute bottom-3 left-3 flex flex-wrap items-center gap-3 rounded-md border border-default bg-elevated/90 px-2.5 py-1.5 text-label text-muted shadow-sm empty:hidden" @click.stop @pointerdown.stop>
      <slot name="legend" />
    </div>
    <div class="absolute right-3 bottom-3 flex flex-col overflow-hidden rounded-md border border-default bg-elevated/90 shadow-sm" @click.stop @pointerdown.stop>
      <UButton icon="i-lucide-plus" size="xs" color="neutral" variant="ghost" class="rounded-none" :aria-label="t('CodeGraph.zoomIn')" :title="t('CodeGraph.zoomIn')" @click="zoomBy(1.25)" />
      <UButton icon="i-lucide-minus" size="xs" color="neutral" variant="ghost" class="rounded-none" :aria-label="t('CodeGraph.zoomOut')" :title="t('CodeGraph.zoomOut')" @click="zoomBy(0.8)" />
      <UButton icon="i-lucide-maximize" size="xs" color="neutral" variant="ghost" class="rounded-none border-t border-default" :aria-label="t('CodeGraph.fit')" :title="t('CodeGraph.fit')" @click="fit" />
    </div>
  </div>
</template>
