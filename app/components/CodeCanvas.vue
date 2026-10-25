<script setup lang="ts">
import { boxAt, boxesById, bounds, canvasTree, insets, layout, type Box } from '~~/shared/canvas'

/*
 * The canvas: the code as boxes in boxes, drawn on a plain 2D canvas. Scroll to
 * zoom, drag to move, click a function to open it, click any other box (or double-click) to zoom to it. Zooming in
 * shows more: parts and their names far out, then folders and files, then function names, and at the end the first
 * lines of a function's code. Boxes too small to see are not drawn, so thousands of functions stay smooth. A function
 * hovered or picked shows its calls as lines to the boxes they reach (dashed when matched by name only).
 *
 * The Map lens and Code use the same canvas: `changed` functions are amber and the functions that call them
 * (`affected`) outlined, `heat` tints files by how often they changed, and `focus` zooms to a few files.
 */
const props = defineProps<{
  changed?: Set<string>
  affected?: Set<string>
  /** Per file path, a 0–1 score of how often it changed. */
  heat?: Map<string, number> | null
  /** File paths to zoom to when they change. */
  focus?: string[]
  /** Function names to mark (a search). */
  term?: string
}>()

const { structure: scan, index, selected, select, visible } = useCode()
const { repo } = useRepositories()
const colorMode = useColorMode()

const WORLD = { x: 0, y: 0, w: 1000, h: 625 }
const tree = computed(() => {
  if (!scan.value) return null
  const root = canvasTree(scan.value.files.filter(visible), scan.value.parts, scan.value.layers)
  layout(root, WORLD)
  return { root, byId: boxesById(root) }
})

const wrapper = ref<HTMLElement>()
const canvas = ref<HTMLCanvasElement>()
let context: CanvasRenderingContext2D | null = null
let size = { w: 0, h: 0, ratio: 1 }
// The view: screen = world × k + t.
const view = { k: 1, tx: 0, ty: 0 }
let fitted = false
const hovered = shallowRef<Box | null>(null)
const tip = ref<{ x: number, y: number, title: string, detail: string } | null>(null)

/* Colours: the theme's, read once per theme (a canvas reads no CSS variables). */
let colors: Record<string, string> = {}
const layerColors = new Map<string, string>()
function readColors(): void {
  colors = {
    bg: resolvedColor('--ui-bg'),
    elevated: resolvedColor('--ui-bg-elevated'),
    border: resolvedColor('--ui-border'),
    accented: resolvedColor('--ui-border-accented'),
    text: resolvedColor('--ui-text'),
    muted: resolvedColor('--ui-text-muted'),
    dimmed: resolvedColor('--ui-text-dimmed'),
    primary: resolvedColor('--ui-primary'),
    warning: resolvedColor('--ui-warning'),
    info: resolvedColor('--ui-info'),
    error: resolvedColor('--ui-error')
  }
  layerColors.clear()
}
function layerColor(layer: string): string {
  let found = layerColors.get(layer)
  if (!found) {
    found = resolvedColor(layerVar(layer))
    layerColors.set(layer, found)
  }
  return found
}

/* Function code, read when a function's box is big enough to show it. */
const code = new Map<string, string[] | null>()
let reading = 0
/** Bumped with every scan: code asked for before it lands nowhere. */
let codeScan = 0
function codeOf(item: Box): string[] | null | undefined {
  if (code.has(item.id)) return code.get(item.id)
  if (!repo.value || !item.fn || !item.path || reading >= 4) return undefined
  code.set(item.id, null)
  reading++
  const at = repo.value
  const asked = codeScan
  haunt.source(at, item.path, item.fn.line, Math.min(item.fn.endLine, item.fn.line + 80))
    .then((found) => { if (asked === codeScan) code.set(item.id, found.lines) })
    .catch(() => { if (asked === codeScan) code.set(item.id, []) })
    .finally(() => {
      reading--
      if (repo.value === at) redraw()
    })
  return null
}
watch(() => scan.value?.scannedAt, () => {
  codeScan++
  code.clear()
})

/* Drawing */
const EXPAND = { w: 28, h: 20 }
const screen = (item: Box) => ({ x: item.x * view.k + view.tx, y: item.y * view.k + view.ty, w: item.w * view.k, h: item.h * view.k })
const expanded = (item: Box) => item.w * view.k > EXPAND.w && item.h * view.k > EXPAND.h

function drawBox(ctx: CanvasRenderingContext2D, item: Box, term: string, chosen: string | null): void {
  const s = screen(item)
  if (s.x > size.w || s.y > size.h || s.x + s.w < 0 || s.y + s.h < 0 || s.w < 1.5 || s.h < 1.5) return
  const layer = layerColor(item.layer)
  let fill: string
  if (item.kind === 'part') fill = alpha(layer, 0.1)
  else if (item.kind === 'folder') fill = alpha(layer, 0.06)
  else if (item.kind === 'file') fill = colors.elevated!
  else fill = alpha(layer, 0.26 + Math.min(0.24, (item.depth % 4) * 0.06))
  ctx.fillStyle = fill
  ctx.fillRect(s.x, s.y, s.w, s.h)
  if (item.kind === 'file' && props.heat) {
    const score = props.heat.get(item.path!) ?? 0
    if (score > 0) {
      ctx.fillStyle = alpha(colors.error!, 0.08 + score * 0.4)
      ctx.fillRect(s.x, s.y, s.w, s.h)
    }
  }
  const isChanged = item.kind === 'function' && props.changed?.has(item.id)
  if (isChanged) {
    ctx.fillStyle = alpha(colors.warning!, 0.55)
    ctx.fillRect(s.x, s.y, s.w, s.h)
  }
  ctx.lineWidth = 1
  ctx.strokeStyle = item.kind === 'part' ? alpha(layer, 0.6) : colors.border!
  ctx.strokeRect(s.x + 0.5, s.y + 0.5, s.w - 1, s.h - 1)
  if (item.kind === 'function' && !isChanged && props.affected?.has(item.id)) {
    ctx.save()
    ctx.setLineDash([4, 3])
    ctx.lineWidth = 1.5
    ctx.strokeStyle = colors.warning!
    ctx.strokeRect(s.x + 1, s.y + 1, s.w - 2, s.h - 2)
    ctx.restore()
  }
  const matched = term && item.kind === 'function' && item.label.toLowerCase().includes(term)
  if (matched || item.id === chosen || item === hovered.value) {
    ctx.lineWidth = item.id === chosen ? 2.5 : 1.5
    ctx.strokeStyle = item === hovered.value && item.id !== chosen && !matched ? colors.accented! : colors.primary!
    ctx.strokeRect(s.x + 1, s.y + 1, s.w - 2, s.h - 2)
  }

  // Its name, in the space kept above its children (a function without children: at its top).
  const { top, side } = insets(item)
  const header = item.children.length ? top * view.k : Math.min(s.h, 22)
  if (s.w > 36 && header >= 8) {
    const fontSize = Math.max(9, Math.min(item.kind === 'part' ? 15 : 13, header * 0.72))
    ctx.font = `${item.kind === 'part' ? '600 ' : item.kind === 'function' ? '500 ' : ''}${fontSize}px ${item.kind === 'function' ? 'ui-monospace, monospace' : 'system-ui, sans-serif'}`
    ctx.fillStyle = item.kind === 'folder' ? colors.muted! : colors.text!
    ctx.textBaseline = 'middle'
    const pad = Math.max(3, side * view.k)
    fitText(ctx, item.label, s.x + pad, s.y + Math.min(header, 22) / 2 + 1, s.w - pad * 2)
  }

  if (item.children.length && expanded(item)) {
    for (const child of item.children) drawBox(ctx, child, term, chosen)
  } else if (item.kind === 'function' && !item.children.length && s.w > 220 && s.h > 110) {
    drawCode(ctx, item, s)
  }
}

function drawCode(ctx: CanvasRenderingContext2D, item: Box, s: { x: number, y: number, w: number, h: number }): void {
  const lines = codeOf(item)
  if (lines) codeLines(ctx, lines, s, colors.muted!)
}

/** The box a call line reaches: the box itself, or its nearest ancestor drawn at this zoom. */
function anchor(item: Box): Box {
  const chain: Box[] = []
  for (let at: Box | null = item; at && at.kind !== 'root'; at = at.parent) chain.unshift(at)
  let found = chain[0]!
  for (const next of chain.slice(1)) {
    if (!expanded(found) || next.w * view.k < 1.5 || next.h * view.k < 1.5) break
    found = next
  }
  return found
}

function drawCalls(ctx: CanvasRenderingContext2D, from: Box): void {
  const at = index.value
  const byId = tree.value?.byId
  if (!at || !byId) return
  const source = anchor(from)
  const s = screen(source)
  const start = { x: s.x + s.w / 2, y: s.y + s.h / 2 }
  const line = (target: Box, guessed: boolean, color: string) => {
    const end = anchor(target)
    if (end === source) return
    const e = screen(end)
    curvedArrow(ctx, start, { x: e.x + e.w / 2, y: e.y + e.h / 2 }, color, guessed)
  }
  for (const edge of (at.callees.get(from.id) ?? []).slice(0, 150)) {
    const target = byId.get(edge.to)
    if (target) line(target, !!edge.guessed, colors.primary!)
  }
  for (const edge of (at.callers.get(from.id) ?? []).slice(0, 150)) {
    const caller = byId.get(edge.from)
    if (caller) line(caller, !!edge.guessed, colors.info!)
  }
}

let frame = 0
function redraw(): void {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const ctx = context
    const current = tree.value
    if (!ctx || !current) return
    ctx.setTransform(size.ratio, 0, 0, size.ratio, 0, 0)
    ctx.fillStyle = colors.bg!
    ctx.fillRect(0, 0, size.w, size.h)
    const chosen = selected.value?.fn.id ?? null
    const term = (props.term ?? '').trim().toLowerCase()
    for (const part of current.root.children) drawBox(ctx, part, term, chosen)
    const from = hovered.value?.kind === 'function' ? hovered.value : chosen ? current.byId.get(chosen) : null
    if (from) drawCalls(ctx, from)
    drawMap()
  })
}

/* The view */
function fitScale(): number {
  return Math.min(size.w / WORLD.w, size.h / WORLD.h) * 0.98
}
function fit(): void {
  view.k = fitScale()
  view.tx = (size.w - WORLD.w * view.k) / 2
  view.ty = (size.h - WORLD.h * view.k) / 2
  redraw()
}
let animation = 0
/** Zooms smoothly so `rect` fills the view. */
function zoomTo(rect: { x: number, y: number, w: number, h: number }): void {
  const k = Math.min(fitScale() * 4000, Math.min(size.w / rect.w, size.h / rect.h) * 0.85)
  const target = { k, tx: size.w / 2 - (rect.x + rect.w / 2) * k, ty: size.h / 2 - (rect.y + rect.h / 2) * k }
  const from = { ...view }
  const started = performance.now()
  cancelAnimationFrame(animation)
  const step = (now: number) => {
    const t = Math.min(1, (now - started) / 280)
    const ease = 1 - (1 - t) ** 3
    // The scale moves evenly on a log scale; the point at the centre moves with it.
    view.k = Math.exp(Math.log(from.k) + (Math.log(target.k) - Math.log(from.k)) * ease)
    const cx = (size.w / 2 - from.tx) / from.k + ((size.w / 2 - target.tx) / target.k - (size.w / 2 - from.tx) / from.k) * ease
    const cy = (size.h / 2 - from.ty) / from.k + ((size.h / 2 - target.ty) / target.k - (size.h / 2 - from.ty) / from.k) * ease
    view.tx = size.w / 2 - cx * view.k
    view.ty = size.h / 2 - cy * view.k
    redraw()
    if (t < 1) animation = requestAnimationFrame(step)
  }
  animation = requestAnimationFrame(step)
}
function zoomToIds(ids: string[]): void {
  const byId = tree.value?.byId
  if (!byId) return
  const rect = bounds(ids.map(id => byId.get(id)).filter((item): item is Box => !!item))
  if (rect) zoomTo(rect)
}

function worldAt(event: { clientX: number, clientY: number }): { x: number, y: number } {
  const r = canvas.value!.getBoundingClientRect()
  return { x: (event.clientX - r.left - view.tx) / view.k, y: (event.clientY - r.top - view.ty) / view.k }
}

function onWheel(event: WheelEvent): void {
  const r = canvas.value!.getBoundingClientRect()
  const px = event.clientX - r.left
  const py = event.clientY - r.top
  const factor = Math.exp(-event.deltaY * 0.0015)
  const k = Math.min(fitScale() * 4000, Math.max(fitScale() * 0.8, view.k * factor))
  view.tx = px - ((px - view.tx) / view.k) * k
  view.ty = py - ((py - view.ty) / view.k) * k
  view.k = k
  cancelAnimationFrame(animation)
  redraw()
}

let drag: { x: number, y: number, tx: number, ty: number, moved: boolean } | null = null
function onDown(event: PointerEvent): void {
  if (event.button !== 0) return
  canvas.value!.setPointerCapture(event.pointerId)
  drag = { x: event.clientX, y: event.clientY, tx: view.tx, ty: view.ty, moved: false }
}
function onMove(event: PointerEvent): void {
  if (drag) {
    const dx = event.clientX - drag.x
    const dy = event.clientY - drag.y
    if (Math.abs(dx) + Math.abs(dy) > 4) drag.moved = true
    if (drag.moved) {
      view.tx = drag.tx + dx
      view.ty = drag.ty + dy
      cancelAnimationFrame(animation)
      tip.value = null
      redraw()
      return
    }
  }
  const current = tree.value
  if (!current) return
  const point = worldAt(event)
  const found = boxAt(current.root, point.x, point.y, view.k)
  if (found !== hovered.value) {
    hovered.value = found
    redraw()
  }
  const r = wrapper.value!.getBoundingClientRect()
  tip.value = found
    ? {
        x: event.clientX - r.left,
        y: event.clientY - r.top,
        title: found.kind === 'folder' ? found.label : found.label,
        detail: [
          found.kind === 'function' ? `${found.path}:${found.fn!.line}` : found.kind === 'file' ? found.path! : found.kind,
          `${found.weight} lines`,
          found.kind === 'function' && index.value
            ? `${index.value.callers.get(found.id)?.length ?? 0} callers, calls ${index.value.callees.get(found.id)?.length ?? 0}`
            : null
        ].filter(Boolean).join(' · ')
      }
    : null
}
function onUp(event: PointerEvent): void {
  const wasDrag = drag?.moved
  drag = null
  if (wasDrag || !tree.value) return
  const point = worldAt(event)
  const found = boxAt(tree.value.root, point.x, point.y, view.k)
  if (!found) return
  if (found.kind === 'function') select(found.id)
  else zoomTo(found)
}
function onDouble(event: MouseEvent): void {
  if (!tree.value) return
  const point = worldAt(event)
  const found = boxAt(tree.value.root, point.x, point.y, view.k)
  if (found) zoomTo(found)
}
function onLeave(): void {
  hovered.value = null
  tip.value = null
  redraw()
}

let observer: ResizeObserver | null = null
onMounted(() => {
  context = canvas.value!.getContext('2d')
  readColors()
  observer = new ResizeObserver(() => {
    const r = wrapper.value!.getBoundingClientRect()
    const ratio = window.devicePixelRatio || 1
    const firstSize = size.w === 0
    // The point at the centre stays at the centre.
    const cx = size.w ? (size.w / 2 - view.tx) / view.k : 0
    const cy = size.h ? (size.h / 2 - view.ty) / view.k : 0
    size = { w: r.width, h: r.height, ratio }
    canvas.value!.width = Math.round(r.width * ratio)
    canvas.value!.height = Math.round(r.height * ratio)
    if (firstSize || !fitted) {
      fit()
      fitted = size.w > 0
      if (props.focus?.length) zoomToIds(props.focus.map(path => `file:${path}`))
      else if (selected.value) zoomToIds([selected.value.fn.id])
    } else {
      view.tx = size.w / 2 - cx * view.k
      view.ty = size.h / 2 - cy * view.k
      redraw()
    }
  })
  observer.observe(wrapper.value!)
})
onBeforeUnmount(() => {
  observer?.disconnect()
  cancelAnimationFrame(frame)
  cancelAnimationFrame(animation)
})

watch(tree, redraw)
watch(() => [selected.value, props.changed, props.affected, props.heat, props.term, index.value], redraw)
watch(() => colorMode.value, () => nextTick(() => {
  readColors()
  redraw()
}))
// The search zooms to the first function it finds (after a pause in typing).
let searching: ReturnType<typeof setTimeout> | undefined
watch(() => props.term, (now) => {
  clearTimeout(searching)
  const term = (now ?? '').trim().toLowerCase()
  if (term.length < 2) return
  searching = setTimeout(() => {
    const first = [...tree.value?.byId.values() ?? []].find(item => item.kind === 'function' && item.label.toLowerCase().includes(term))
    if (first) zoomTo(first)
  }, 400)
})

/* The overview map: the parts, and the part of the canvas in view; click or drag on it to move there. */
const MAP = { w: 168, h: 105 }
const mapCanvas = ref<HTMLCanvasElement>()
function drawMap(): void {
  const ctx = mapCanvas.value?.getContext('2d')
  const current = tree.value
  if (!ctx || !current || !size.w) return
  const ratio = size.ratio
  // Setting a canvas's size clears it and makes its pixels anew: only when the screen's density changed.
  const width = MAP.w * ratio
  if (mapCanvas.value!.width !== width) {
    mapCanvas.value!.width = width
    mapCanvas.value!.height = MAP.h * ratio
  }
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0)
  const k = Math.min(MAP.w / WORLD.w, MAP.h / WORLD.h)
  ctx.fillStyle = colors.elevated!
  ctx.fillRect(0, 0, MAP.w, MAP.h)
  for (const part of current.root.children) {
    ctx.fillStyle = alpha(layerColor(part.layer), 0.45)
    ctx.fillRect(part.x * k, part.y * k, Math.max(1, part.w * k), Math.max(1, part.h * k))
  }
  // What is in view, in world units.
  const x = -view.tx / view.k
  const y = -view.ty / view.k
  ctx.strokeStyle = colors.primary!
  ctx.lineWidth = 1.5
  ctx.strokeRect(x * k, y * k, (size.w / view.k) * k, (size.h / view.k) * k)
}
function mapTo(event: PointerEvent): void {
  if (event.type === 'pointermove' && !(event.buttons & 1)) return
  const r = mapCanvas.value!.getBoundingClientRect()
  const k = Math.min(MAP.w / WORLD.w, MAP.h / WORLD.h)
  const wx = (event.clientX - r.left) / k
  const wy = (event.clientY - r.top) / k
  view.tx = size.w / 2 - wx * view.k
  view.ty = size.h / 2 - wy * view.k
  redraw()
}

watch(() => props.focus, (now) => {
  if (now?.length && fitted) zoomToIds(now.map(path => `file:${path}`))
})

defineExpose({ fit, zoomToIds })
</script>

<template>
  <div ref="wrapper" class="relative size-full min-h-80 overflow-hidden">
    <canvas
      ref="canvas"
      class="absolute inset-0 size-full cursor-default touch-none"
      :class="{ 'cursor-pointer': hovered }"
      @wheel.prevent="onWheel"
      @pointerdown="onDown"
      @pointermove="onMove"
      @pointerup="onUp"
      @dblclick="onDouble"
      @pointerleave="onLeave"
    />
    <canvas
      ref="mapCanvas"
      class="absolute right-2 bottom-2 cursor-crosshair rounded border border-default shadow-sm"
      :style="{ width: `${MAP.w}px`, height: `${MAP.h}px` }"
      title="Where you are: click or drag to move"
      @pointerdown="mapTo"
      @pointermove="mapTo"
    />
    <div
      v-if="tip"
      class="pointer-events-none absolute z-10 max-w-80 rounded-md border border-default bg-default/95 px-2 py-1 text-xs shadow-sm"
      :style="{ left: `${Math.min(tip.x + 14, (size.w || 0) - 200)}px`, top: `${tip.y + 16}px` }"
    >
      <p class="truncate font-mono font-medium text-highlighted">{{ tip.title }}</p>
      <p class="truncate text-muted">{{ tip.detail }}</p>
    </div>
  </div>
</template>
