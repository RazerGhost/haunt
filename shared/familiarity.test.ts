import assert from 'node:assert/strict'
import { test } from 'node:test'
import { familiarKey, familiarity, hashCode, isRecords, percent, tally, tallyTree, type FamiliarRecord } from './familiarity.ts'

const code = ['export function store(order) {', '  if (!order.id) throw new Error()', '  return save(order)', '}']

test('names a function by its file and the names it is written in', () => {
  assert.equal(familiarKey('src/order.ts', ['Order.store']), 'src/order.ts#Order.store')
  assert.equal(familiarKey('src/a.ts', ['outer', 'inner']), 'src/a.ts#outer>inner')
})

test('the hash keeps through indentation and blank lines, not through a change', () => {
  const moved = ['    export function store(order) {', '', '      if (!order.id) throw new Error()  ', '      return save(order)', '    }']
  assert.equal(hashCode(moved, 4), hashCode(code))
  assert.notEqual(hashCode(['export function store(order) {', '  return save(order)', '}']), hashCode(code))
  assert.notEqual(hashCode([...code.slice(0, 2), '  return keep(order)', '}']), hashCode(code))
})

test('a function read in part is told apart from a longer one', () => {
  assert.notEqual(hashCode(code, 4), hashCode(code, 900))
})

test('familiar while the code is as it was, stale once it changed, nothing when never learned', () => {
  const record: FamiliarRecord = { at: 1, source: 'manual', hash: hashCode(code) }
  assert.equal(familiarity(record, hashCode(code)), 'familiar')
  assert.equal(familiarity(record, hashCode(['changed'])), 'stale')
  assert.equal(familiarity(record, undefined), 'familiar')
  assert.equal(familiarity(undefined, hashCode(code)), null)
})

test('reads back only well-formed records', () => {
  assert.equal(isRecords({ 'a.ts#f': { at: 1, source: 'route-recall', hash: 'x' } }), true)
  assert.equal(isRecords({ 'a.ts#f': { at: 1, source: 'guess', hash: 'x' } }), false)
  assert.equal(isRecords([]), false)
  assert.equal(isRecords(null), false)
})

test('counts the familiar and the stale functions of a set and of every box of a tree', () => {
  const states = new Map([['a', 'familiar'], ['b', 'stale'], ['c', 'familiar']] as const)
  assert.deepEqual(tally(['a', 'b', 'x', 'y'], states), { total: 4, familiar: 1, stale: 1 })
  type Node = { id: string, kind: string, children: Node[] }
  const fn = (id: string, children: Node[] = []): Node => ({ id, kind: 'function', children })
  const root: Node = { id: 'root', kind: 'root', children: [
    { id: 'file:one', kind: 'file', children: [fn('a', [fn('c')]), fn('x')] },
    { id: 'file:two', kind: 'file', children: [fn('b')] }
  ] }
  const shares = tallyTree(root, states)
  assert.deepEqual(shares.get('a'), { total: 2, familiar: 2, stale: 0 })
  assert.deepEqual(shares.get('file:one'), { total: 3, familiar: 2, stale: 0 })
  assert.deepEqual(shares.get('root'), { total: 4, familiar: 2, stale: 1 })
  assert.equal(percent(shares.get('file:one')!), 67)
  assert.equal(percent({ total: 0, familiar: 0, stale: 0 }), 0)
})
