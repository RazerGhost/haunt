import assert from 'node:assert/strict'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import ts from 'typescript'
import { readConfig } from './config.ts'
import { importsIn, innerImports, runtimeCycles } from './imports.ts'
import { scanner } from './scan.ts'
import { globToRegExp } from './workspaces.ts'
import type { Scan } from '../shared/types.ts'

/* Haunt's own import graph, and the layer rules of a haunt.config.json. */

const fixture = (name: string) => fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url))

/** The imports between parts as `from → to [breaks] (types)`, sorted. */
function summary(scan: Scan): string[] {
  return scan.imports.map((group) => {
    const breaks = [...new Set(group.files.flatMap(file => file.breaks))].sort()
    const types = group.files.every(file => file.typeOnly) ? ' (types)' : ''
    return `${group.from} → ${group.to}${breaks.length ? ` [${breaks.join(', ')}]` : ''}${types}`
  }).sort()
}

test('judges imports by the layers, uses and entries of haunt.config.json', async () => {
  const scan = await scanner(fixture('layered')).structure()
  assert.equal(scan.rules, 'config')
  assert.deepEqual(scan.layers.map(layer => [layer.id, layer.label, !!layer.ruled]), [['app', 'Application', true], ['domain', 'domain', true], ['infra', 'infra', true]])
  assert.deepEqual(scan.parts.map(part => [part.name, part.layer, part.folder]).sort(), [
    ['app', 'app', 'src/app'],
    ['db', 'infra', 'src/infra/db'],
    ['domain/billing', 'domain', 'src/domain/billing'],
    ['domain/orders', 'domain', 'src/domain/orders'],
    ['domain/shared', 'domain', 'src/domain/shared']
  ])
  assert.deepEqual(scan.parts.find(part => part.name === 'db')?.role, 'storage')
  assert.deepEqual(scan.parts.find(part => part.name === 'domain/orders')?.uses, ['domain/shared'])
  assert.deepEqual(summary(scan), [
    'app → domain/orders', // a lower layer, through its entry
    'domain/billing → domain/orders [layer]', // same layer, not in uses
    'domain/orders → app [door, layer] (types)', // a higher layer, if only for a type; and app has no index.ts
    'domain/orders → db', // a lower layer, through its entry (which re-exports save.ts)
    'domain/orders → domain/billing [door, layer]', // not in uses, and past its entry
    'domain/orders → domain/shared' // same layer, in uses
  ])
  assert.deepEqual(scan.cycles, [['src/domain/billing/format.ts', 'src/domain/orders/index.ts']])
})

test('reads and judges the imports of another version of a file, against the files as they are now', async () => {
  const scan = scanner(fixture('layered'))
  await scan.structure()
  const found = scan.imports([
    // save.ts reaching up into a higher layer, by a relative path and past its entry.
    { key: 'new', path: 'src/infra/db/save.ts', text: ["import { placeOrder } from '../../domain/orders'", "import type { Ui } from '../../app/ui-types'", 'export const x = placeOrder'].join('\n') },
    { key: 'old', path: 'src/infra/db/save.ts', text: 'export const x = 1' },
    { key: 'missing', path: 'src/infra/db/save.ts', text: "import './nowhere'" }
  ])
  assert.equal(found.judged, true)
  assert.deepEqual(found.files.old, [])
  assert.deepEqual(found.files.missing, [], 'an import of no scanned file is left out')
  assert.deepEqual(found.files.new!.map(edge => [edge.to, edge.fromPart, edge.toPart, edge.typeOnly, [...edge.breaks].sort()]), [
    ['src/domain/orders/index.ts', 'db', 'domain/orders', false, ['layer']],
    ['src/app/ui-types.ts', 'db', 'app', true, ['door', 'layer']]
  ])
  // Without rules nothing is judged.
  const loose = scanner(fixture('monorepo'))
  await loose.structure()
  assert.equal(loose.imports([]).judged, false)
})

test('draws the imports between workspace packages without rules', async () => {
  const scan = await scanner(fixture('monorepo')).structure()
  assert.equal(scan.rules, null)
  assert.deepEqual(summary(scan), ['@demo/ui → @demo/core', '@demo/web → @demo/core'])
  assert.deepEqual(scan.cycles, [])
})

test('reads what an import brings in', () => {
  const sf = ts.createSourceFile('x.ts', [
    'import type { A } from "./types"',
    'import { type B, type C } from "./more-types"',
    'import { type D, e } from "./mixed"',
    'import "./side-effect"',
    'export * from "./everything"',
    'export type { F } from "./reexported-types"',
    'const lazy = () => import("./lazy")',
    'import legacy = require("./legacy")'
  ].join('\n'), ts.ScriptTarget.Latest, true)
  assert.deepEqual(importsIn(sf, ts), [
    { specifier: './types', typeOnly: true },
    { specifier: './more-types', typeOnly: true },
    { specifier: './mixed', typeOnly: false },
    { specifier: './side-effect', typeOnly: false },
    { specifier: './everything', typeOnly: false },
    { specifier: './reexported-types', typeOnly: true },
    { specifier: './lazy', typeOnly: false },
    { specifier: './legacy', typeOnly: false }
  ])
})

test('a cycle needs runtime imports both ways', () => {
  assert.deepEqual(runtimeCycles(['a', 'b', 'c'], [
    { from: 'a', to: 'b', typeOnly: false },
    { from: 'b', to: 'a', typeOnly: true },
    { from: 'b', to: 'c', typeOnly: false },
    { from: 'c', to: 'b', typeOnly: false }
  ]), [['b', 'c']])
})

test('names parts from the globs of a rule', () => {
  assert.deepEqual(globToRegExp('src/domain/*', true).exec('src/domain/orders')?.slice(1), ['orders'])
  assert.deepEqual(globToRegExp('apps/*/src/**', true).exec('apps/web/src/pages/home')?.slice(1), ['web', 'pages/home'])
})

test('says what is wrong with a haunt.config.json', () => {
  const dir = mkdtempSync(join(tmpdir(), 'haunt-config-'))
  const write = (config: unknown) => writeFileSync(join(dir, 'haunt.config.json'), typeof config === 'string' ? config : JSON.stringify(config))
  try {
    assert.equal(readConfig(dir), null)
    write('{ "layers": [ }')
    assert.throws(() => readConfig(dir), /is not valid JSON/)
    write({ layers: [{ id: 'app' }], parts: [{ match: 'src/app', layer: 'web' }] })
    assert.throws(() => readConfig(dir), /parts\[0\] names layer "web", which "layers" does not list/)
    write({ layers: [{ id: 'a' }, { id: 'a' }] })
    assert.throws(() => readConfig(dir), /two layers have the same id/)
    write({ layers: [{ id: 'app' }], parts: [{ match: 'src/app', layer: 'app' }], entries: ['index.ts'] })
    assert.deepEqual(readConfig(dir), { layers: [{ id: 'app' }], parts: [{ match: 'src/app', layer: 'app' }], entries: ['index.ts'], architecture: null })
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('keeps the imports within a part apart from the ones between parts', () => {
  const partOf = (path: string) => path.split('/')[0]!
  const edges = [
    { from: 'core/a.ts', to: 'core/b.ts', typeOnly: false },
    { from: 'core/a.ts', to: 'api/x.ts', typeOnly: true },
    { from: 'api/x.ts', to: 'api/y.ts', typeOnly: true }
  ]
  assert.deepEqual(innerImports(edges, partOf), [
    { from: 'core/a.ts', to: 'core/b.ts', typeOnly: false },
    { from: 'api/x.ts', to: 'api/y.ts', typeOnly: true }
  ])
})
