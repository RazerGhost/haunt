import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import { ownTypescript } from './compiler.ts'

/* Which TypeScript of a project can read it: a 5.x or 6.x has the compiler API, TypeScript 7 (the native compiler)
 * exports only its version. */

const haunt = dirname(dirname(fileURLToPath(import.meta.url)))

test('a TypeScript with the compiler API (Haunt\'s own 6.x here) is the project\'s to use', () => {
  const own = ownTypescript(haunt)
  assert.ok(own)
  assert.match(own.version, /^[56]\./)
  assert.deepEqual(own.missing, [])
})

test('TypeScript 7\'s package, with only its version, lacks the API; a folder without TypeScript has none', () => {
  const dir = mkdtempSync(join(tmpdir(), 'haunt-compiler-'))
  try {
    writeFileSync(join(dir, 'package.json'), '{"name":"x"}')
    assert.equal(ownTypescript(dir), null)
    mkdirSync(join(dir, 'node_modules', 'typescript'), { recursive: true })
    writeFileSync(join(dir, 'node_modules', 'typescript', 'package.json'), JSON.stringify({ name: 'typescript', version: '7.0.2', main: 'index.js' }))
    writeFileSync(join(dir, 'node_modules', 'typescript', 'index.js'), 'module.exports = { version: \'7.0.2\', versionMajorMinor: \'7.0\' }\n')
    const own = ownTypescript(dir)
    assert.equal(own?.version, '7.0.2')
    assert.ok(own?.missing.includes('getParsedCommandLineOfConfigFile'))
    assert.ok(own?.missing.includes('createProgram'))
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
