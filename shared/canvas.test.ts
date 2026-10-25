import assert from 'node:assert/strict'
import { test } from 'node:test'
import { boxAt, boxesById, canvasTree, layout, orderedSquarify, type Box } from './canvas.ts'
import type { FunctionNode, Layer, Part, ScannedFile } from './types.ts'

/* The canvas: boxes in boxes, sized by lines, in an order that keeps their places. */

const fn = (id: string, line: number, endLine: number, children: FunctionNode[] = []): FunctionNode =>
  ({ id, name: id, kind: 'function', line, endLine, exported: true, async: false, anonymous: false, children })
const file = (path: string, part: string, lines: number, functions: FunctionNode[] = []): ScannedFile =>
  ({ path, part, test: false, lines, functions })

const layers: Layer[] = [{ id: 'entry', label: 'Entry', note: '' }, { id: 'kernel', label: 'Kernel', note: '' }]
const parts: Part[] = [
  { name: 'kernel', layer: 'kernel', role: '', uses: [], folder: 'src/kernel' },
  { name: 'api', layer: 'entry', role: '', uses: [], folder: 'apps/api' }
]
const files = [
  file('src/kernel/errors.ts', 'kernel', 40, [fn('errors.ts:1:1', 1, 30)]),
  file('src/kernel/db/client.ts', 'kernel', 100, [fn('outer', 1, 60, [fn('inner', 10, 30)]), fn('later', 70, 90)]),
  file('apps/api/src/server/http/routes.ts', 'api', 60)
]

const inside = (child: Box, parent: Box) =>
  child.x >= parent.x - 1e-9 && child.y >= parent.y - 1e-9 && child.x + child.w <= parent.x + parent.w + 1e-9 && child.y + child.h <= parent.y + parent.h + 1e-9
const overlap = (a: Box, b: Box) => a.x < b.x + b.w - 1e-9 && b.x < a.x + a.w - 1e-9 && a.y < b.y + b.h - 1e-9 && b.y < a.y + a.h - 1e-9

test('builds parts in the order of their layers, folders, files and functions inside functions', () => {
  const root = canvasTree(files, parts, layers)
  assert.deepEqual(root.children.map(item => item.label), ['api', 'kernel'], 'entry comes before kernel')
  const byId = boxesById(root)
  // A folder holding only one folder is one box.
  assert.equal(byId.get('folder:api:src/server/http/')?.label, 'src/server/http/')
  const client = byId.get('file:src/kernel/db/client.ts')!
  assert.deepEqual(client.children.map(item => item.id), ['outer', 'later'])
  assert.equal(byId.get('inner')?.parent?.id, 'outer')
  assert.equal(client.weight, 100)
  assert.equal(client.rest, 100 - 60 - 21, 'the lines no function covers stay free')
  assert.equal(byId.get('outer')!.rest, 60 - 21)
  assert.equal(byId.get('part:kernel')!.weight, 140)
})

test('lays every box inside its parent, apart from its siblings, sized by its lines', () => {
  const root = canvasTree(files, parts, layers)
  layout(root, { x: 0, y: 0, w: 1000, h: 625 })
  const walk = (item: Box) => {
    item.children.forEach((child, at) => {
      assert.ok(inside(child, item), `${child.id} is inside ${item.id}`)
      for (const other of item.children.slice(at + 1)) assert.ok(!overlap(child, other), `${child.id} and ${other.id} do not overlap`)
      walk(child)
    })
  }
  walk(root)
  const [api, kernel] = root.children
  const ratio = (kernel!.w * kernel!.h) / (api!.w * api!.h)
  assert.ok(Math.abs(ratio - 140 / 60) < 0.2, `kernel's area is to api's as 140 to 60 (${ratio.toFixed(2)})`)
})

test('keeps the order of the boxes, so they stay in place when one grows a little', () => {
  const rect = { x: 0, y: 0, w: 100, h: 60 }
  const before = orderedSquarify([30, 20, 20, 10, 10, 5], rect)
  const after = orderedSquarify([31, 20, 20, 10, 10, 5], rect)
  assert.deepEqual(before[0], { ...before[0], x: 0, y: 0 }, 'the first box starts at the top left')
  const area = (r: { w: number, h: number }) => r.w * r.h
  const total = before.reduce((sum, r) => sum + area(r), 0)
  assert.ok(Math.abs(total - 6000) < 1e-6, 'the boxes fill the space')
  before.forEach((r, at) => {
    assert.ok(Math.abs(r.x - after[at]!.x) < 3 && Math.abs(r.y - after[at]!.y) < 3, `box ${at} moved only a little`)
  })
  // Nothing to lay out: empty boxes, not a crash.
  assert.deepEqual(orderedSquarify([0, 0], rect).map(area), [0, 0])
})

test('finds the deepest box at a point that is big enough on screen', () => {
  const root = canvasTree(files, parts, layers)
  layout(root, { x: 0, y: 0, w: 1000, h: 625 })
  const inner = boxesById(root).get('inner')!
  const x = inner.x + inner.w / 2
  const y = inner.y + inner.h / 2
  assert.equal(boxAt(root, x, y, 100)?.id, 'inner')
  // Zoomed far out, the function is too small to point at: its part answers.
  assert.equal(boxAt(root, x, y, 0.01, 6)?.kind, 'part')
  assert.equal(boxAt(root, -5, -5, 1), null)
})
