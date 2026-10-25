import assert from 'node:assert/strict'
import { test } from 'node:test'
import { busyFiles, riskOf } from './risk.ts'

test('a small, tested change in quiet files is low', () => {
  const risk = riskOf({ changed: 2, unreached: 0, callers: 1, busy: [], days: 180 })
  assert.equal(risk.level, 'low')
  assert.deepEqual(risk.reasons, ['1 function calls what changed', 'tests reach every changed function'])
})

test('callers, untested functions and busy files add up', () => {
  const medium = riskOf({ changed: 4, unreached: 1, callers: 6, busy: [], days: 180 })
  assert.equal(medium.level, 'medium')
  assert.equal(medium.reasons[1], '1 of 4 changed functions no test reaches')

  const high = riskOf({ changed: 3, unreached: 2, callers: 25, busy: [{ path: 'src/api/entries.ts', commits: 31 }], days: 180 })
  assert.equal(high.level, 'high')
  assert.equal(high.points, 5)
  assert.equal(high.reasons[2], 'touches a busy file (entries.ts, 31 commits in 180 days)')
})

test('functions in a language whose tests Haunt does not read are left out of the test count', () => {
  const risk = riskOf({ changed: 5, unreached: 1, unread: 3, callers: 0, busy: [], days: 180 })
  assert.deepEqual(risk.reasons, ['1 of 2 changed functions no test reaches', 'tests not read for 3 (PHP, Rust or Lua)'])
  assert.equal(risk.points, 2)
  assert.deepEqual(riskOf({ changed: 2, unreached: 0, unread: 2, callers: 0, busy: [], days: 180 }).reasons, ['tests not read for 2 (PHP, Rust or Lua)'])
})

test('busy files are the busiest tenth, eight changes or more', () => {
  const commits: Record<string, { commits: number }> = {}
  for (let n = 1; n <= 20; n++) commits[`f${n}.ts`] = { commits: n }
  assert.deepEqual(busyFiles(['f20.ts', 'f19.ts', 'f10.ts', 'f2.ts'], commits).map(item => item.path), ['f20.ts', 'f19.ts'])
  // In a quiet repository nothing is busy.
  assert.deepEqual(busyFiles(['a.ts'], { 'a.ts': { commits: 3 }, 'b.ts': { commits: 1 } }), [])
  assert.deepEqual(busyFiles(['a.ts'], {}), [])
})
