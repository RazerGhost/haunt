import assert from 'node:assert/strict'
import { test } from 'node:test'
import { splitRows } from './splitRows.ts'

/* A hunk's lines side by side. */

const line = (kind: 'add' | 'del' | 'context', text: string) => ({ kind, text })

test('context is on both sides, the same line', () => {
  const a = line('context', 'a')
  assert.deepEqual(splitRows([a]), [{ left: a, right: a }])
})

test('removals next to the additions after them, blank on the shorter side', () => {
  const lines = [line('del', 'x'), line('del', 'y'), line('add', 'X'), line('context', 'c'), line('add', 'z')]
  assert.deepEqual(splitRows(lines).map(row => [row.left?.text ?? null, row.right?.text ?? null]), [
    ['x', 'X'],
    ['y', null],
    ['c', 'c'],
    [null, 'z']
  ])
})

test('additions before removals are not paired with them', () => {
  const lines = [line('add', 'a'), line('del', 'd')]
  assert.deepEqual(splitRows(lines).map(row => [row.left?.text ?? null, row.right?.text ?? null]), [
    [null, 'a'],
    ['d', null]
  ])
})

test('every line is in a row once, on its own side', () => {
  const lines = [line('del', '1'), line('add', '2'), line('add', '3'), line('del', '4'), line('context', '5')]
  const rows = splitRows(lines)
  assert.equal(rows.filter(row => row.left?.kind === 'del').length, 2)
  assert.equal(rows.filter(row => row.right?.kind === 'add').length, 2)
  assert.ok(rows.every(row => row.left?.kind !== 'add' && row.right?.kind !== 'del'))
})
