import assert from 'node:assert/strict'
import { test } from 'node:test'
import { columnLayout, type ColumnNode } from './columnLayout.ts'

/* Cards in columns that mean something, ordered so their lines cross less. */

const node = (id: string, column: number, h = 40): ColumnNode => ({ id, column, w: 300, h })

test('columns go left to right by their number, each centred on the middle', () => {
  const placed = columnLayout([node('b', 1), node('a', -1), node('c', 0, 100)], [], { gap: 20, columnGap: 100 })
  assert.deepEqual([placed.get('a')!.x, placed.get('c')!.x, placed.get('b')!.x], [0, 400, 800])
  assert.equal(placed.get('c')!.y, -50)
  assert.equal(placed.get('a')!.y, -20)
})

test('cards move towards the cards they are joined to, so lines cross less', () => {
  // Left: top, bottom. Right, given crossed: joined to bottom first, to top second.
  const placed = columnLayout(
    [node('top', 0), node('bottom', 0), node('toBottom', 1), node('toTop', 1)],
    [{ source: 'top', target: 'toTop' }, { source: 'bottom', target: 'toBottom' }]
  )
  assert.ok(placed.get('toTop')!.y < placed.get('toBottom')!.y)
  assert.ok(placed.get('top')!.y < placed.get('bottom')!.y)
})

test('a column taller than the room wraps into several side by side', () => {
  const nodes = Array.from({ length: 6 }, (_, at) => node(`n${at}`, 0, 100))
  const placed = columnLayout([...nodes, node('next', 1)], [], { gap: 20, columnGap: 100, maxHeight: 300 })
  const xs = [...new Set(nodes.map(item => placed.get(item.id)!.x))]
  assert.deepEqual(xs, [0, 340, 680])
  assert.equal(placed.get('next')!.x, 1080)
})

test('links to nodes not laid out, and to themselves, are left alone', () => {
  const placed = columnLayout([node('a', 0)], [{ source: 'a', target: 'gone' }, { source: 'a', target: 'a' }])
  assert.deepEqual(placed.get('a'), { x: 0, y: -20, w: 300, h: 40 })
})
