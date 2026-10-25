import assert from 'node:assert/strict'
import { test } from 'node:test'
import { testsReaching } from './testReach.ts'

// add ← twice ← main; add ← "adds" (test); twice ← "doubles" (test) ← nothing; lonely has no callers.
const callers: Record<string, string[]> = {
  add: ['twice', 'adds'],
  twice: ['main', 'doubles'],
  main: [],
  adds: [],
  doubles: [],
  lonely: []
}
const of = (id: string) => callers[id] ?? []
const isTest = (id: string) => id === 'adds' || id === 'doubles'

test('finds the tests that reach each changed function, nearest first', () => {
  const reach = testsReaching(['add'], of, isTest)
  assert.deepEqual(reach.tests.map(item => [item.id, [...item.reaches]]), [['adds', [['add', 1]]], ['doubles', [['add', 2]]]])
  assert.deepEqual(reach.unreached, [])
})

test('lists the changed functions no test reaches, and keeps to the depth', () => {
  assert.deepEqual(testsReaching(['lonely', 'main'], of, isTest).unreached, ['lonely', 'main'])
  assert.deepEqual(testsReaching(['add'], of, isTest, 1).tests.map(item => item.id), ['adds'])
})

test('a changed test reaches its own change, and one test can reach several', () => {
  const reach = testsReaching(['adds', 'add', 'twice'], of, isTest)
  const adds = reach.tests.find(item => item.id === 'adds')!
  assert.deepEqual([...adds.reaches], [['adds', 0], ['add', 1]])
  assert.deepEqual([...reach.tests.find(item => item.id === 'doubles')!.reaches], [['add', 2], ['twice', 1]])
})

test('a cycle in the calls ends', () => {
  const cyclic: Record<string, string[]> = { a: ['b'], b: ['a'] }
  assert.deepEqual(testsReaching(['a'], id => cyclic[id] ?? [], () => false).unreached, ['a'])
})
