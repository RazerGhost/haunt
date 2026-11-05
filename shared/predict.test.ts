import assert from 'node:assert/strict'
import { test } from 'node:test'
import { candidates, grade, puzzle } from './predict.ts'

// route → store → save → db; route → keep → save → db (another way down); cli → db (direct); job → store → save → db.
const paths = [
  ['route', 'store', 'save', 'db'],
  ['route', 'keep', 'save', 'db'],
  ['cli', 'db'],
  ['job', 'store', 'save', 'db']
]
const calls = new Set(['route>store', 'route>keep', 'store>save', 'keep>save', 'save>db', 'cli>db', 'job>store'])
const callsOf = (from: string, to: string) => calls.has(`${from}>${to}`)

test('numbers the places between the entry points and the function, a middle seen before only once', () => {
  const found = puzzle(paths)
  assert.deepEqual(found.paths.map(path => path.join(' ')), ['route store save db', 'route keep save db'])
  assert.deepEqual(found.slots.map(slot => [slot.id, slot.real, slot.number]), [['0:1', 'store', 1], ['0:2', 'save', 2], ['1:1', 'keep', 3], ['1:2', 'save', 4]])
})

test('keeps at most `max` paths', () => {
  assert.equal(puzzle(paths, 1).paths.length, 1)
})

test('deals the real functions once each among decoys, the same way for the same seed', () => {
  const pool = ['a', 'b', 'c', 'd', 'e', 'f', 'store']
  const list = candidates(['store', 'save', 'keep', 'save'], pool, 7)
  assert.equal(list.length, 3 + 4)
  for (const id of ['store', 'save', 'keep']) assert.equal(list.filter(item => item === id).length, 1)
  assert.deepEqual(candidates(['store', 'save', 'keep'], pool, 7), list)
  assert.equal(candidates(['x'], ['a'], 1).length, 2)
})

test('a pick is right when it is the function there, or fits there as well', () => {
  const found = puzzle(paths)
  const picks = new Map([['0:1', 'keep'], ['0:2', 'store'], ['1:1', 'keep']])
  const marks = grade(found, picks, callsOf)
  // keep fits between route and save; store does not call db; place 1:2 is still empty.
  assert.deepEqual([...marks], [['0:1', true], ['0:2', false], ['1:1', true]])
})
