import assert from 'node:assert/strict'
import { test } from 'node:test'
import { testSupport } from './testSupport.ts'

// Two tests call checkPage, which calls el; checkPage calls render too, which the app's main calls as well.
// loopA and loopB only call each other; lonely calls nothing and nothing calls it.
const calls: [string, string][] = [
  ['test:a', 'checkPage'],
  ['test:b', 'checkPage'],
  ['checkPage', 'el'],
  ['checkPage', 'render'],
  ['main', 'render'],
  ['loopA', 'loopB'],
  ['loopB', 'loopA']
]
const callersOf = (id: string) => calls.filter(([, to]) => to === id).map(([from]) => from)
const calleesOf = (id: string) => calls.filter(([from]) => from === id).map(([, to]) => to)
const ids = ['test:a', 'test:b', 'checkPage', 'el', 'render', 'main', 'loopA', 'loopB', 'lonely']
const inTestFile = (id: string) => id.startsWith('test:')

test('takes in the tests and the helpers only tests reach', () => {
  const found = testSupport(ids, inTestFile, callersOf, calleesOf)
  assert.deepEqual([...found].sort(), ['checkPage', 'el', 'test:a', 'test:b'])
})

test('keeps out what the app calls too, what nothing calls, and cycles nothing reaches', () => {
  const found = testSupport(ids, inTestFile, callersOf, calleesOf)
  for (const id of ['render', 'main', 'lonely', 'loopA', 'loopB']) assert.equal(found.has(id), false, id)
})

test('a helper called by a test file and by a helper the app uses stays out', () => {
  const more: [string, string][] = [...calls, ['main', 'el']]
  const found = testSupport(ids, inTestFile, id => more.filter(([, to]) => to === id).map(([from]) => from), id => more.filter(([from]) => from === id).map(([, to]) => to))
  assert.equal(found.has('el'), false)
  assert.equal(found.has('checkPage'), true)
})
