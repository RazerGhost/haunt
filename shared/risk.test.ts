import assert from 'node:assert/strict'
import { test } from 'node:test'
import { busyFiles, riskOf } from './risk.ts'

test('a small, tested change in quiet files is low', () => {
  const risk = riskOf({ changed: 2, unreached: 0, callers: 1, busy: [], days: 180 })
  assert.equal(risk.level, 'low')
  assert.deepEqual(risk.reasons, [{ kind: 'callers', count: 1 }, { kind: 'reached', count: 2, partly: false }])
})

test('callers, untested functions and busy files add up', () => {
  const medium = riskOf({ changed: 4, unreached: 1, callers: 6, busy: [], days: 180 })
  assert.equal(medium.level, 'medium')
  assert.deepEqual(medium.reasons[1], { kind: 'unreached', count: 1, of: 4 })

  const high = riskOf({ changed: 3, unreached: 2, callers: 25, busy: [{ path: 'src/api/entries.ts', commits: 31 }], days: 180 })
  assert.equal(high.level, 'high')
  assert.equal(high.points, 5)
  assert.deepEqual(high.reasons[2], { kind: 'busy', count: 1, name: 'entries.ts', commits: 31, days: 180 })
})

test('functions in a language whose tests Haunt does not read are left out of the test count', () => {
  const risk = riskOf({ changed: 5, unreached: 1, unread: 3, callers: 0, busy: [], days: 180 })
  assert.deepEqual(risk.reasons, [{ kind: 'unreached', count: 1, of: 2 }, { kind: 'unread', count: 3 }])
  assert.equal(risk.points, 2)
  assert.deepEqual(riskOf({ changed: 2, unreached: 0, unread: 2, callers: 0, busy: [], days: 180 }).reasons, [{ kind: 'unread', count: 2 }])
  assert.deepEqual(riskOf({ changed: 5, unreached: 0, unread: 3, callers: 0, busy: [], days: 180 }).reasons, [{ kind: 'reached', count: 2, partly: true }, { kind: 'unread', count: 3 }])
})

test('busy files are the busiest tenth, eight changes or more', () => {
  const commits: Record<string, { commits: number }> = {}
  for (let n = 1; n <= 20; n++) commits[`f${n}.ts`] = { commits: n }
  assert.deepEqual(busyFiles(['f20.ts', 'f19.ts', 'f10.ts', 'f2.ts'], commits).map(item => item.path), ['f20.ts', 'f19.ts'])
  // In a quiet repository nothing is busy.
  assert.deepEqual(busyFiles(['a.ts'], { 'a.ts': { commits: 3 }, 'b.ts': { commits: 1 } }), [])
  assert.deepEqual(busyFiles(['a.ts'], {}), [])
})
