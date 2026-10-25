import assert from 'node:assert/strict'
import { test } from 'node:test'
import { entryPoints, pathView, pathsTo } from './callPaths.ts'

/* How a function is reached: the shortest paths from the entry points, and what it calls, in ordered rows. */

// route → controller → service → createEntry → insert
//                    ↘ validate ↗           ↘ audit
// script → createEntry
// loopA ⇄ loopB → orphan
const calls: [string, string][] = [
  ['route', 'controller'],
  ['controller', 'service'],
  ['controller', 'validate'],
  ['service', 'createEntry'],
  ['validate', 'createEntry'],
  ['script', 'createEntry'],
  ['createEntry', 'insert'],
  ['createEntry', 'audit'],
  ['loopA', 'loopB'],
  ['loopB', 'loopA'],
  ['loopB', 'orphan']
]
const callers = (id: string) => calls.filter(([, to]) => to === id).map(([from]) => from)
const callees = (id: string) => calls.filter(([from]) => from === id).map(([, to]) => to)

test('finds the shortest paths from the entry points, each from the entry point down', () => {
  const paths = pathsTo('createEntry', callers)
  assert.deepEqual(paths, [
    ['script', 'createEntry'],
    ['route', 'controller', 'service', 'createEntry'],
    ['route', 'controller', 'validate', 'createEntry']
  ])
  assert.deepEqual(pathsTo('createEntry', callers, 1), [['script', 'createEntry']], 'at most `limit` paths')
  // A function left out (a test, say) is not an entry point and does not lead anywhere.
  assert.deepEqual(pathsTo('createEntry', callers, 12, id => id !== 'script').length, 2)
})

test('lays the paths out in rows above the function and what it calls below it', () => {
  const view = pathView('createEntry', callers, callees)
  assert.deepEqual(view.rows.map(row => [row.step, row.ids]), [
    [-3, ['route']],
    [-2, ['controller']],
    [-1, ['script', 'service', 'validate']],
    [0, ['createEntry']],
    [1, ['audit', 'insert']]
  ])
  assert.deepEqual(view.entries.sort(), ['route', 'script'])
  assert.equal(view.paths, 3)
  assert.equal(view.shortest, 1)
  assert.equal(view.longest, 3)
  assert.ok(view.edges.some(edge => edge.from === 'controller' && edge.to === 'validate'))
  assert.ok(view.edges.some(edge => edge.from === 'createEntry' && edge.to === 'insert'))
  assert.equal(view.isEntry, false)
})

test('says when a function is an entry point, or reached only through a cycle', () => {
  const route = pathView('route', callers, callees)
  assert.equal(route.isEntry, true)
  assert.deepEqual(route.rows.map(row => row.step), [0, 1])
  const orphan = pathView('orphan', callers, callees)
  assert.equal(orphan.cyclic, true)
  assert.deepEqual(orphan.rows.find(row => row.step === -1)?.ids, ['loopB'])
  assert.equal(orphan.paths, 0)
})

test('lists the entry points: called by nothing, calling something, the busiest first', () => {
  const ids = [...new Set(calls.flat())]
  assert.deepEqual(entryPoints(ids, callers, callees), ['route', 'script'])
})
