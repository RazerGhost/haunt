import assert from 'node:assert/strict'
import { test } from 'node:test'
import { importGraph, reaches } from './reach.ts'

test('a new import closes a cycle when its target already reaches it, through run-time imports only', () => {
  const graph = importGraph([
    { from: 'a', to: 'b', typeOnly: false },
    { from: 'b', to: 'c', typeOnly: false },
    { from: 'c', to: 'd', typeOnly: true }
  ])
  // c importing a: a → b → c is there, so c → a closes it.
  assert.equal(reaches(graph, 'a', 'c'), true)
  // d importing a: a reaches d only through a type-only import, so no cycle.
  assert.equal(reaches(graph, 'a', 'd'), false)
  assert.equal(reaches(graph, 'x', 'a'), false)
})
