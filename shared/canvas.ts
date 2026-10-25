import { folderTree, type Folder } from './folders.ts'
import type { FunctionNode, Layer, Part, ScannedFile } from './types.ts'
import { addTo } from './collect.ts'

/*
 * The canvas's boxes in boxes: a part holds its folders, a folder its files, a
 * file its functions, and a function the functions written inside it. Each box is sized by its lines of code and laid
 * out by Haunt, not by a force layout, as a treemap inside its parent that keeps its children in a fixed order (by
 * name, by line), so a box stays where it was between scans and moves only a little when the code around it grows.
 * Coordinates are the canvas's own ("world") units; the window zooms and pans over them.
 */

export type BoxKind = 'root' | 'part' | 'folder' | 'file' | 'function'

export interface Box {
  /** `part:<name>`, `folder:<part>:<path>`, `file:<path>`, or the function's own id. */
  id: string
  kind: BoxKind
  label: string
  /** Lines of code, what its area stands for. */
  weight: number
  /** The box's own lines that none of its children cover (a file's imports, a function's body around the functions
   * written in it): kept free in the layout, so a child's area matches its lines. */
  rest: number
  children: Box[]
  parent: Box | null
  /** The band of its part, for its colour. */
  layer: string
  /** A file's path; a function's file. */
  path: string | null
  fn: FunctionNode | null
  depth: number
  x: number
  y: number
  w: number
  h: number
}

function box(id: string, kind: BoxKind, label: string, layer: string, parent: Box | null): Box {
  return { id, kind, label, weight: 0, rest: 0, children: [], parent, layer, path: null, fn: null, depth: parent ? parent.depth + 1 : 0, x: 0, y: 0, w: 0, h: 0 }
}

function functionBox(fn: FunctionNode, path: string, layer: string, parent: Box): Box {
  const item = box(fn.id, 'function', fn.anonymous ? '(callback)' : fn.name, layer, parent)
  item.path = path
  item.fn = fn
  item.children = fn.children.map(child => functionBox(child, path, layer, item))
  const lines = Math.max(1, fn.endLine - fn.line + 1)
  const inner = item.children.reduce((sum, child) => sum + child.weight, 0)
  item.weight = Math.max(lines, inner)
  item.rest = item.weight - inner
  return item
}

function fileBox(file: ScannedFile, layer: string, parent: Box): Box {
  const item = box(`file:${file.path}`, 'file', file.path.split('/').pop()!, layer, parent)
  item.path = file.path
  item.children = file.functions.map(fn => functionBox(fn, file.path, layer, item))
  const inner = item.children.reduce((sum, child) => sum + child.weight, 0)
  item.weight = Math.max(1, file.lines, inner)
  item.rest = item.weight - inner
  return item
}

/** A folder's box; a folder holding only one folder and no files is shown as one box ("server/http/"). */
function folderBox(folder: Folder, part: string, layer: string, parent: Box): Box {
  let at = folder
  let label = `${folder.name}/`
  while (at.files.length === 0 && at.folders.length === 1) {
    at = at.folders[0]!
    label += `${at.name}/`
  }
  const item = box(`folder:${part}:${at.path}`, 'folder', label, layer, parent)
  fillFolder(item, at, part, layer)
  return item
}

function fillFolder(item: Box, folder: Folder, part: string, layer: string): void {
  item.children = [
    ...folder.folders.map(sub => folderBox(sub, part, layer, item)),
    ...folder.files.map(file => fileBox(file, layer, item))
  ]
  item.weight = Math.max(1, item.children.reduce((sum, child) => sum + child.weight, 0))
}

/** The boxes of a scan: its parts in the order of their layers (then by name), with the files `visible` lets in. */
export function canvasTree(files: ScannedFile[], parts: Part[], layers: Layer[]): Box {
  const root = box('root', 'root', '', 'unmapped', null)
  const order = new Map(layers.map((layer, at) => [layer.id, at]))
  const byPart = new Map<string, ScannedFile[]>()
  for (const file of files) addTo(byPart, file.part, file)
  const known = new Map(parts.map(part => [part.name, part]))
  const names = [...byPart.keys()].sort((a, b) => {
    const layerA = order.get(known.get(a)?.layer ?? '') ?? layers.length
    const layerB = order.get(known.get(b)?.layer ?? '') ?? layers.length
    return layerA - layerB || a.localeCompare(b)
  })
  root.children = names.map((name) => {
    const part = known.get(name)
    const layer = part?.layer ?? 'unmapped'
    const item = box(`part:${name}`, 'part', name, layer, root)
    fillFolder(item, folderTree(byPart.get(name)!, part?.folder ?? '.'), name, layer)
    return item
  })
  root.weight = Math.max(1, root.children.reduce((sum, child) => sum + child.weight, 0))
  return root
}

/** The space a box keeps for its name above its children, and around them, as a share of its own size. */
export function insets(item: Box): { top: number, side: number } {
  const short = Math.min(item.w, item.h)
  if (item.kind === 'root') return { top: 0, side: 0 }
  return { top: Math.min(item.h * 0.22, short * 0.09 + 0.6), side: short * 0.02 }
}

interface Rect { x: number, y: number, w: number, h: number }

/**
 * Lays `weights` out in `rect` as a treemap that keeps their order: each row (or column, along the shorter side) takes
 * the next weights while that makes its boxes squarer (Bruls, Huizing and van Wijk's squarified treemap, without the
 * sorting by size, so the order and with it the places stay).
 */
export function orderedSquarify(weights: number[], rect: Rect): Rect[] {
  const total = weights.reduce((sum, weight) => sum + weight, 0)
  const out: Rect[] = weights.map(() => ({ x: rect.x, y: rect.y, w: 0, h: 0 }))
  if (total <= 0 || rect.w <= 0 || rect.h <= 0) return out
  const scale = (rect.w * rect.h) / total
  let free = { ...rect }
  let row: number[] = []
  const worst = (items: number[], side: number) => {
    const areas = items.map(at => weights[at]! * scale)
    const sum = areas.reduce((a, b) => a + b, 0)
    const max = Math.max(...areas)
    const min = Math.min(...areas)
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min))
  }
  const place = (items: number[]) => {
    const sum = items.reduce((a, at) => a + weights[at]! * scale, 0)
    const across = free.w >= free.h
    if (across) {
      // A column on the left, as wide as its area needs.
      const width = free.h > 0 ? sum / free.h : 0
      let y = free.y
      for (const at of items) {
        const h = width > 0 ? (weights[at]! * scale) / width : 0
        out[at] = { x: free.x, y, w: width, h }
        y += h
      }
      free = { x: free.x + width, y: free.y, w: free.w - width, h: free.h }
    } else {
      const height = free.w > 0 ? sum / free.w : 0
      let x = free.x
      for (const at of items) {
        const w = height > 0 ? (weights[at]! * scale) / height : 0
        out[at] = { x, y: free.y, w, h: height }
        x += w
      }
      free = { x: free.x, y: free.y + height, w: free.w, h: free.h - height }
    }
  }
  for (let at = 0; at < weights.length; at++) {
    if (weights[at]! <= 0) continue
    const side = Math.min(free.w, free.h)
    if (row.length === 0 || worst([...row, at], side) <= worst(row, side)) {
      row.push(at)
    } else {
      place(row)
      row = [at]
    }
  }
  if (row.length) place(row)
  return out
}

/** Places every box: the root fills `rect`, and each box's children (and its own free lines) fill it inside its insets. */
export function layout(root: Box, rect: Rect): void {
  Object.assign(root, rect)
  const walk = (item: Box) => {
    if (!item.children.length) return
    const { top, side } = insets(item)
    const inner = { x: item.x + side, y: item.y + top, w: Math.max(0, item.w - side * 2), h: Math.max(0, item.h - top - side) }
    // The box's own lines take the last place, so the children start at the top left and keep their order.
    const weights = [...item.children.map(child => child.weight), item.rest]
    const rects = orderedSquarify(weights, inner)
    item.children.forEach((child, at) => {
      // A small gap between siblings, so neighbours read as separate boxes.
      const r = rects[at]!
      const gap = Math.min(r.w, r.h) * 0.03
      Object.assign(child, { x: r.x + gap / 2, y: r.y + gap / 2, w: Math.max(0, r.w - gap), h: Math.max(0, r.h - gap) })
      walk(child)
    })
  }
  walk(root)
}

/** Every box by id. */
export function boxesById(root: Box): Map<string, Box> {
  const out = new Map<string, Box>()
  const walk = (item: Box) => {
    out.set(item.id, item)
    item.children.forEach(walk)
  }
  walk(root)
  return out
}

/** The deepest box at a point whose size on screen (`scale` pixels per unit) is at least `min` pixels. */
export function boxAt(root: Box, x: number, y: number, scale: number, min = 6): Box | null {
  let found: Box | null = null
  let at: Box | undefined = root
  while (at) {
    found = at.kind === 'root' ? null : at
    at = at.children.find(child => child.w * scale >= min && child.h * scale >= min && x >= child.x && x <= child.x + child.w && y >= child.y && y <= child.y + child.h)
  }
  return found
}

/** The rectangle around a few boxes, for zooming to them. */
export function bounds(items: Box[]): Rect | null {
  if (!items.length) return null
  const x = Math.min(...items.map(item => item.x))
  const y = Math.min(...items.map(item => item.y))
  const right = Math.max(...items.map(item => item.x + item.w))
  const bottom = Math.max(...items.map(item => item.y + item.h))
  return { x, y, w: right - x, h: bottom - y }
}
