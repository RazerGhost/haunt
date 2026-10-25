import assert from 'node:assert/strict'
import { test } from 'node:test'
import { changedSpans, markSpans } from './wordDiff.ts'

/* The words that changed within a changed line. */

test('marks only the words that changed', () => {
  const { before, after } = changedSpans('  return total + 1', '  return total + tax')
  assert.deepEqual(before, [{ start: 17, end: 18 }])
  assert.deepEqual(after, [{ start: 17, end: 20 }])
})

test('an unchanged line has nothing marked, a new word only on its side', () => {
  assert.deepEqual(changedSpans('a b', 'a b'), { before: [], after: [] })
  assert.deepEqual(changedSpans('call(a)', 'call(a, b)'), { before: [], after: [{ start: 6, end: 9 }] })
})

test('splits styled pieces at the marks', () => {
  const pieces = [{ content: 'return ', color: 'k' }, { content: 'total', color: 'v' }]
  assert.deepEqual(markSpans(pieces, [{ start: 9, end: 12 }]), [
    { content: 'return ', color: 'k', marked: false },
    { content: 'to', color: 'v', marked: false },
    { content: 'tal', color: 'v', marked: true }
  ])
})
