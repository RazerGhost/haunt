import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import ts from 'typescript'
import { detectProject } from './project.ts'
import { scanner } from './scan.ts'
import { globToRegExp, pnpmPatterns, workspacePackages } from './workspaces.ts'
import type { FunctionNode } from '../shared/types.ts'

/* Any TypeScript project without configuration, on the fixture projects in sidecar/fixtures. */

const fixture = (name: string) => fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url))

test('a single package: the files its tsconfig includes, parts by folder in src/', async () => {
  const project = await detectProject(fixture('single'), ts, false)
  assert.equal(project.kind, 'typescript')
  assert.equal(project.name, 'single-app')
  // scripts/release.ts is outside tsconfig.json's include.
  assert.deepEqual(project.files, ['src/index.ts', 'src/math/add.test.ts', 'src/math/add.ts', 'src/ui/Button.tsx'])
  assert.deepEqual(project.locate('src/math/add.ts'), { part: 'math', folder: 'src/math', layer: 'parts' })
  assert.deepEqual(project.locate('src/index.ts'), { part: 'src', folder: 'src', layer: 'parts' })
  assert.deepEqual(project.layers.map(layer => layer.id), ['parts'])
  assert.equal(project.isTest('src/math/add.test.ts'), true)
  assert.equal(project.isTest('src/math/add.ts'), false)
  assert.deepEqual(project.watch, ['src'])
})

test('a pnpm workspace: references and package tsconfigs, a part per package, a band per folder', async () => {
  const project = await detectProject(fixture('monorepo'), ts, false)
  assert.equal(project.name, 'demo-monorepo')
  // apps/web is not referenced by the root tsconfig, but is a workspace package; packages/ignored is left out by '!'.
  assert.deepEqual(project.files, ['apps/web/src/main.ts', 'packages/core/src/index.ts', 'packages/core/src/internal/format.ts', 'packages/ui/src/Card.tsx'])
  assert.deepEqual(project.locate('packages/core/src/internal/format.ts'), { part: '@demo/core', folder: 'packages/core', layer: 'packages' })
  assert.deepEqual(project.locate('apps/web/src/main.ts'), { part: '@demo/web', folder: 'apps/web', layer: 'apps' })
  assert.deepEqual(project.layers.map(layer => layer.id), ['apps', 'packages'])
  // The options of the first project with files: the path alias of tsconfig.base.json, through `extends`.
  assert.ok(project.options.paths?.['@demo/core'])
})

test('a folder without a tsconfig: every TypeScript file outside builds and dependencies', async () => {
  const project = await detectProject(fixture('loose'), ts, false)
  assert.deepEqual(project.files, ['index.ts', 'lib/a.ts', 'lib/b.ts'])
  assert.deepEqual(project.locate('lib/a.ts'), { part: 'lib', folder: 'lib', layer: 'parts' })
  assert.deepEqual(project.locate('index.ts'), { part: '(root)', folder: '', layer: 'parts' })
})

test('scans a workspace: .tsx functions, and calls across packages through a path alias', async () => {
  const scan = scanner(fixture('monorepo'))
  const structure = await scan.structure()
  assert.deepEqual(structure.project, { kind: 'typescript', name: 'demo-monorepo' })
  assert.deepEqual(structure.parts.map(part => [part.name, part.layer]).sort(), [['@demo/core', 'packages'], ['@demo/ui', 'packages'], ['@demo/web', 'apps']])
  const names = new Map<string, string>()
  const walk = (items: FunctionNode[]) => items.forEach((item) => {
    names.set(item.id, item.name)
    walk(item.children)
  })
  structure.files.forEach(file => walk(file.functions))
  assert.ok([...names.values()].includes('Card'))
  const calls = scan.calls().calls.map(edge => `${names.get(edge.from)} → ${names.get(edge.to)}`).sort()
  assert.deepEqual(calls, ['Card → greet', 'greet → shout', 'start → greet'])
})

test('reads workspace patterns', () => {
  assert.deepEqual(pnpmPatterns('packages:\n  - \'packages/*\'\n  - apps/*   # the apps\n  - "!packages/old"\nonlyBuiltDependencies:\n  - esbuild\n'), ['packages/*', 'apps/*', '!packages/old'])
  assert.deepEqual(pnpmPatterns('packages: [a/*, "b"]'), ['a/*', 'b'])
  assert.equal(globToRegExp('packages/*').test('packages/core'), true)
  assert.equal(globToRegExp('packages/*').test('packages/core/sub'), false)
  assert.equal(globToRegExp('apps/**').test('apps/web/admin'), true)
  assert.equal(globToRegExp('**/pkg').test('deep/down/pkg'), true)
  assert.equal(globToRegExp('**/pkg').test('pkg'), true)
  assert.equal(globToRegExp('./core').test('core'), true)
  assert.deepEqual(workspacePackages(fixture('monorepo')).map(item => item.name), ['@demo/web', '@demo/core', '@demo/ui'])
})
