import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { after, test } from 'node:test'
import { fingerprint, readCalls, writeCalls } from './cache.ts'

/* The call graph kept between launches: used again only while what it was worked out from is unchanged. */

const made: string[] = []
after(() => made.forEach(dir => rmSync(dir, { recursive: true, force: true })))
const dir = () => {
  const out = mkdtempSync(join(tmpdir(), 'haunt-cache-'))
  made.push(out)
  return out
}

test('a saved call graph comes back while its files are unchanged, and not after a change', () => {
  const root = dir()
  const cache = dir()
  writeFileSync(join(root, 'a.ts'), 'export function a() { return 1 }\n')
  writeFileSync(join(root, 'tsconfig.json'), '{}\n')
  const calls = [{ from: 'a.ts:1:8', to: 'b.ts:1:8', count: 2 }]
  const key = fingerprint(root, ['a.ts'], false)
  writeCalls(cache, root, key, { calls, uses: { 'a.ts:1:1': 'inside' } })
  assert.deepEqual(readCalls(cache, root, fingerprint(root, ['a.ts'], false)), { calls, uses: { 'a.ts:1:1': 'inside' } })
  // Another trust, another file list, a changed file or a changed tsconfig: worked out again.
  assert.equal(readCalls(cache, root, fingerprint(root, ['a.ts'], true)), null)
  assert.equal(readCalls(cache, root, fingerprint(root, ['a.ts', 'b.ts'], false)), null)
  // A workspace part's own settings count too.
  const withSettings = fingerprint(root, ['a.ts'], false, ['pkg/tsconfig.json'])
  mkdirSync(join(root, 'pkg'), { recursive: true })
  writeFileSync(join(root, 'pkg/tsconfig.json'), '{}')
  assert.notEqual(fingerprint(root, ['a.ts'], false, ['pkg/tsconfig.json']), withSettings)
  const later = new Date(Date.now() + 5000)
  utimesSync(join(root, 'tsconfig.json'), later, later)
  assert.equal(readCalls(cache, root, fingerprint(root, ['a.ts'], false)), null)
  // Without a cache folder (the tests' sidecars), nothing is kept.
  writeCalls(undefined, root, key, { calls, uses: {} })
  assert.equal(readCalls(undefined, root, key), null)
})
