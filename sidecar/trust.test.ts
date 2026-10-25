import assert from 'node:assert/strict'
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import ts from 'typescript'
import { readConfig } from './config.ts'
import { detectProject } from './project.ts'
import { scanner } from './scan.ts'
import type { Scan } from '../shared/types.ts'

/* A project's own rules in code (its architecture script), which run only in a folder you trust. */

const fixture = (name: string) => fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url))
const runs = () => (globalThis as { hauntFixtureRuns?: number }).hauntFixtureRuns ?? 0

function summary(scan: Scan): string[] {
  return scan.imports.map((group) => {
    const breaks = [...new Set(group.files.flatMap(file => file.breaks))].sort()
    const known = group.files.some(file => file.known) ? ' (known)' : ''
    return `${group.from} → ${group.to}${breaks.length ? ` [${breaks.join(', ')}]` : ''}${known}`
  }).sort()
}

test('an untrusted folder: the script does not run, and the project is read without rules', async () => {
  const before = runs()
  const project = await detectProject(fixture('scripted'), ts, false)
  assert.equal(runs(), before)
  assert.equal(project.rules, null)
  assert.equal(project.architectureError, null)
  assert.deepEqual(project.layers.map(layer => layer.id), ['parts'])
  // Its folder is still watched, and a change there starts a new scanner.
  assert.deepEqual(project.restartOn, ['tools'])
  assert.ok(project.watch.includes('tools'))
})

test('a trusted folder: the layers, parts and judge of the script', async () => {
  const before = runs()
  const scan = await scanner(fixture('scripted'), { trusted: true }).structure()
  assert.equal(runs(), before + 1)
  assert.equal(scan.trusted, true)
  assert.equal(scan.rules, 'script')
  assert.deepEqual(scan.layers.map(layer => [layer.id, layer.label, !!layer.ruled]), [['ui', 'Interface', true], ['core', 'core', true], ['parts', 'parts', false]])
  assert.deepEqual(scan.parts.map(part => [part.name, part.layer, part.folder, part.role]).sort(), [
    ['core', 'core', 'src/core', 'adds things up'],
    ['src', 'parts', 'src', ''],
    ['ui', 'ui', 'src/ui', '']
  ])
  assert.deepEqual(summary(scan), ['core → ui [layer]', 'src → ui', 'ui → core'])
})

test('a script may bring its own judged imports and cycles', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'haunt-script-'))
  try {
    mkdirSync(join(dir, 'src/a'), { recursive: true })
    mkdirSync(join(dir, 'src/b'), { recursive: true })
    writeFileSync(join(dir, 'package.json'), '{ "name": "own-imports" }')
    writeFileSync(join(dir, 'src/a/one.ts'), 'export const one = 1\n')
    writeFileSync(join(dir, 'src/b/two.ts'), 'export const two = 2\n')
    writeFileSync(join(dir, 'haunt.config.json'), JSON.stringify({ architecture: { script: 'rules.mjs' } }))
    writeFileSync(join(dir, 'rules.mjs'), `export function architecture() {
      return {
        layers: [{ id: 'top' }, { id: 'bottom' }],
        parts: [{ name: 'a', layer: 'bottom' }, { name: 'b', layer: 'top' }],
        partOf: path => path.startsWith('src/a/') ? 'a' : path.startsWith('src/b/') ? 'b' : null,
        imports: [{ from: 'src/a/one.ts', to: 'src/b/two.ts', breaks: ['layer'], known: true }, { from: 'src/b/two.ts', to: 'src/a/one.ts' }],
        cycles: [['src/a/one.ts', 'src/b/two.ts']]
      }
    }`)
    const scan = await scanner(dir, { trusted: true }).structure()
    assert.equal(scan.rules, 'script')
    assert.deepEqual(summary(scan), ['a → b [layer] (known)', 'b → a'])
    assert.deepEqual(scan.cycles, [['src/a/one.ts', 'src/b/two.ts']])

    // A script that returns the wrong shape says so, and the project falls back to no rules. (Its own file: Node keeps
    // a module once loaded, which is why the server starts a new scanner when a script changes.)
    writeFileSync(join(dir, 'haunt.config.json'), JSON.stringify({ architecture: { script: 'broken.mjs' } }))
    writeFileSync(join(dir, 'broken.mjs'), 'export default () => ({ layers: [{ id: "top" }], parts: [{ name: "a", layer: "nowhere" }], partOf: () => null })')
    const broken = await detectProject(dir, ts, true)
    assert.equal(broken.rules, null)
    assert.match(broken.architectureError ?? '', /must return "parts": \[\{ name, layer \}\], each in one of its layers/)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('the script must be a module inside the project', () => {
  const dir = mkdtempSync(join(tmpdir(), 'haunt-script-path-'))
  const write = (script: string) => writeFileSync(join(dir, 'haunt.config.json'), JSON.stringify({ architecture: { script } }))
  try {
    write('../elsewhere/rules.ts')
    assert.throws(() => readConfig(dir), /must be a path inside the project/)
    write('C:/rules.ts')
    assert.throws(() => readConfig(dir), /must be a path inside the project/)
    write('rules.json')
    assert.throws(() => readConfig(dir), /must be a \.ts, \.mts, \.js or \.mjs file/)
    write('./tools/rules.ts')
    assert.deepEqual(readConfig(dir)?.architecture, { script: 'tools/rules.ts' })
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})
