import assert from 'node:assert/strict'
import { test } from 'node:test'
import { folderAt, folderTree, level } from './folders.ts'
import type { FunctionNode, ScannedFile } from './types.ts'

/* Inside a part: its folder tree, and the imports between the folders and files one level inside a folder. */

const fn = (name: string): FunctionNode => ({ id: name, name, kind: 'function', line: 1, endLine: 2, exported: true, async: false, anonymous: false, children: [] })
const file = (path: string, functions = 1): ScannedFile => ({ path, part: 'core', test: false, lines: 10, functions: Array.from({ length: functions }, (_, i) => fn(`${path}#${i}`)) })

const files = [
  file('src/index.ts'),
  file('src/kernel/errors.ts', 2),
  file('src/features/audit/server/audit.ts', 3),
  file('src/features/audit/shared/types.ts', 0),
  file('src/features/team/server/team.ts', 4)
]

test('builds a part\'s folder tree with counts through every folder', () => {
  const root = folderTree(files, 'src')
  assert.deepEqual(root.folders.map(folder => folder.path), ['features/', 'kernel/'])
  assert.deepEqual(root.files.map(item => item.path), ['src/index.ts'])
  assert.equal(root.functionCount, 10)
  const features = folderAt(root, 'features/')!
  assert.deepEqual(features.folders.map(folder => `${folder.name}:${folder.fileCount}:${folder.functionCount}`), ['audit:2:3', 'team:1:4'])
  assert.equal(folderAt(root, 'features/audit/server/')?.files[0]?.path, 'src/features/audit/server/audit.ts')
  assert.equal(folderAt(root, 'nowhere/'), null)
})

test('adds up the imports one level inside a folder, and finds the cycles among them', () => {
  const root = folderTree(files, 'src')
  const imports = [
    { from: 'src/features/team/server/team.ts', to: 'src/features/audit/server/audit.ts', typeOnly: false },
    { from: 'src/features/team/server/team.ts', to: 'src/features/audit/shared/types.ts', typeOnly: true },
    { from: 'src/features/audit/server/audit.ts', to: 'src/kernel/errors.ts', typeOnly: false },
    { from: 'src/kernel/errors.ts', to: 'src/features/audit/server/audit.ts', typeOnly: false },
    { from: 'src/index.ts', to: 'src/kernel/errors.ts', typeOnly: true }
  ]
  const top = level(root, imports, 'src')
  assert.deepEqual(top.nodes.map(node => `${node.kind}:${node.id}`), ['folder:features/', 'folder:kernel/', 'file:src/index.ts'])
  const edges = Object.fromEntries(top.edges.map(edge => [`${edge.from} → ${edge.to}`, `${edge.count}${edge.typeOnly ? ' types' : ''}${edge.cycle ? ' cycle' : ''}`]))
  // Within features/ the team → audit imports are inside one node at this level, so they do not show.
  assert.deepEqual(edges, {
    'features/ → kernel/': '1 cycle',
    'kernel/ → features/': '1 cycle',
    'src/index.ts → kernel/': '1 types'
  })
  const inFeatures = level(folderAt(root, 'features/')!, imports, 'src')
  assert.deepEqual(inFeatures.edges.map(edge => `${edge.from} → ${edge.to} ${edge.count}${edge.typeOnly ? ' types' : ''}`), ['features/team/ → features/audit/ 2'])
})
