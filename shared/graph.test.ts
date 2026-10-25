import assert from 'node:assert/strict'
import { test } from 'node:test'
import { GraphLayout } from './graph.ts'

/* The commit graph's lanes. */

const c = (sha: string, ...parents: string[]) => ({ sha, parents })

test('a straight history is one lane', () => {
  const rows = new GraphLayout().add([c('c', 'b'), c('b', 'a'), c('a')])
  assert.deepEqual(rows.map(row => row.lane), [0, 0, 0])
  assert.deepEqual(rows[0]!.lines, [{ from: 0, to: 0, part: 'bottom', color: 0 }])
  assert.deepEqual(rows[1]!.lines.map(line => line.part), ['top', 'bottom'])
  // The first commit ends its lane.
  assert.deepEqual(rows[2]!.lines.map(line => line.part), ['top'])
})

test('a merge opens a lane for its second parent, and the branch joins back where it left', () => {
  // m merges f into main; f and b both come from a.
  const rows = new GraphLayout().add([c('m', 'b', 'f'), c('f', 'a'), c('b', 'a'), c('a')])
  assert.deepEqual(rows.map(row => row.lane), [0, 1, 0, 0])
  assert.ok(rows[0]!.lines.some(line => line.from === 0 && line.to === 1 && line.part === 'bottom'))
  // f's parent a: lane 0 does not wait for a yet (it waits for b), so f keeps lane 1 down to a.
  assert.ok(rows[1]!.lines.some(line => line.from === 0 && line.part === 'through'))
  // b continues lane 0 to a; lane 1 also waits for a, and ends in a.
  assert.ok(rows[3]!.lines.some(line => line.from === 1 && line.to === 0 && line.part === 'top'))
  assert.equal(rows[3]!.width, 2)
  assert.notEqual(rows[1]!.color, rows[0]!.color)
})

test('two branch tips get their own lanes, and a tip whose parent another lane waits for joins it', () => {
  const rows = new GraphLayout().add([c('x', 'a'), c('y', 'a'), c('a')])
  assert.deepEqual(rows.map(row => row.lane), [0, 1, 0])
  // y joins lane 0, which already waits for a, rather than keeping a lane of its own.
  assert.ok(rows[1]!.lines.some(line => line.from === 1 && line.to === 0 && line.part === 'bottom'))
  assert.equal(rows[2]!.width, 1)
})

test('the state carries over from page to page', () => {
  const layout = new GraphLayout()
  const first = layout.add([c('m', 'b', 'f'), c('f', 'a')])
  const second = layout.add([c('b', 'a'), c('a')])
  const whole = new GraphLayout().add([c('m', 'b', 'f'), c('f', 'a'), c('b', 'a'), c('a')])
  assert.deepEqual([...first, ...second], whole)
})
