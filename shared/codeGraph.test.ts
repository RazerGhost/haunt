import assert from 'node:assert/strict'
import { test } from 'node:test'
import { assemble, focusGraph, importEdges, pathGraph, projectGraph, reach, type GraphIndex } from './codeGraph.ts'
import type { CallEdge, FunctionNode, Part, ScannedFile } from './types.ts'

/* The code graph: files as cards, functions as rows, calls between the rows. */

function fn(path: string, line: number, name: string, children: FunctionNode[] = [], extra: Partial<FunctionNode> = {}): FunctionNode {
  return { id: `${path}:${line}:1`, name, kind: 'function', line, endLine: line + 5, exported: false, async: false, anonymous: false, children, ...extra }
}

// api/routes.ts: handler            → service.ts: Service.create → db.ts: insert
//                                                 Service.check (nothing calls it)
// api/routes.ts: other (nothing)       db.ts: unused
// test/routes.test.ts: it → handler
const handler = fn('api/routes.ts', 1, 'handler')
const other = fn('api/routes.ts', 10, 'other')
const create = fn('core/service.ts', 2, 'create', [], { kind: 'method' })
const check = fn('core/service.ts', 8, 'check', [], { kind: 'method' })
const service = fn('core/service.ts', 1, 'Service', [create, check], { kind: 'class' })
const insert = fn('core/db.ts', 1, 'insert')
const unused = fn('core/db.ts', 9, 'unused')
const spec = fn('test/routes.test.ts', 1, 'it', [], { test: true })
const files: ScannedFile[] = [
  { path: 'api/routes.ts', part: 'api', test: false, lines: 20, functions: [handler, other] },
  { path: 'core/service.ts', part: 'core', test: false, lines: 20, functions: [service] },
  { path: 'core/db.ts', part: 'core', test: false, lines: 20, functions: [insert, unused] },
  { path: 'test/routes.test.ts', part: 'api', test: true, lines: 5, functions: [spec] }
]
const calls: CallEdge[] = [
  { from: handler.id, to: create.id, count: 2 },
  { from: create.id, to: insert.id, count: 1 },
  { from: spec.id, to: handler.id, count: 1, guessed: true }
]

function indexOf(list: ScannedFile[], edges: CallEdge[]): GraphIndex {
  const functions: GraphIndex['functions'] = new Map()
  const filesByPart = new Map<string, ScannedFile[]>()
  for (const file of list) {
    const go = (items: FunctionNode[], parent: string | null) => items.forEach((item) => {
      functions.set(item.id, { fn: item, file, parent })
      go(item.children, item.id)
    })
    go(file.functions, null)
    filesByPart.set(file.part, [...filesByPart.get(file.part) ?? [], file])
  }
  const callees = new Map<string, CallEdge[]>()
  const callers = new Map<string, CallEdge[]>()
  for (const edge of edges) {
    callees.set(edge.from, [...callees.get(edge.from) ?? [], edge])
    callers.set(edge.to, [...callers.get(edge.to) ?? [], edge])
  }
  return { functions, callees, callers, filesByPart }
}
const index = indexOf(files, calls)
const noTests = { keepFile: (file: ScannedFile) => !file.test }

test('reach weighs the seed most and steps further away less', () => {
  const found = reach(index, [create.id], 2, 'both', () => true)
  assert.equal(found.get(create.id), Infinity)
  assert.equal(found.get(handler.id), 2)
  assert.equal(found.get(insert.id), 1)
  assert.equal(found.get(spec.id), 0.5)
  assert.deepEqual([...reach(index, [create.id], 1, 'callees', () => true).keys()], [create.id, insert.id])
})

test('a function: a card per file reached, with only the rows on an edge and the class as context', () => {
  const graph = focusGraph(index, { kind: 'function', id: create.id }, noTests)
  assert.deepEqual(graph.cards.map(card => card.id), ['file:core/service.ts', 'file:api/routes.ts', 'file:core/db.ts'])
  const home = graph.cards[0]!
  assert.equal(home.home, true)
  assert.deepEqual(home.rows.map(row => [row.id, row.depth, row.context]), [[service.id, 0, true], [create.id, 1, false]])
  // Service.check is folded away; the class is context, not counted as folded.
  assert.equal(home.folded, 2)
  assert.deepEqual(graph.cards[1]!.rows.map(row => row.id), [handler.id])
  assert.equal(graph.cards[1]!.folded, 1)
  const pairs = graph.edges.map(edge => `${edge.from} ${edge.to} ${edge.count}`).sort()
  assert.deepEqual(pairs, [`${create.id} ${insert.id} 1`, `${handler.id} ${create.id} 2`].sort())
})

test('test files are left out unless kept, and guessed calls stay marked', () => {
  const graph = focusGraph(index, { kind: 'function', id: handler.id }, {})
  const edge = graph.edges.find(item => item.from === spec.id)!
  assert.equal(edge.guessed, true)
  assert.ok(!focusGraph(index, { kind: 'function', id: handler.id }, noTests).cards.some(card => card.name.startsWith('test/')))
})

test('opening a card shows all its functions; its new rows bring their edges', () => {
  const graph = focusGraph(index, { kind: 'function', id: handler.id }, { ...noTests, opened: new Set(['file:core/service.ts']) })
  const card = graph.cards.find(item => item.id === 'file:core/service.ts')!
  assert.equal(card.open, true)
  assert.deepEqual(card.rows.map(row => row.id), [service.id, create.id, check.id])
  assert.equal(card.folded, 0)
})

test('a file: its card open, the files its functions call and are called from beside it', () => {
  const graph = focusGraph(index, { kind: 'file', path: 'core/service.ts' }, noTests)
  const home = graph.cards.find(card => card.home)!
  assert.equal(home.open, true)
  assert.deepEqual(home.rows.map(row => row.id), [service.id, create.id, check.id])
  assert.deepEqual(graph.cards.filter(card => !card.home).map(card => card.name).sort(), ['api/routes.ts', 'core/db.ts'])
})

test('a part: every file of it, rows only where there is an edge, the files outside as rows of their part\'s card', () => {
  const graph = focusGraph(index, { kind: 'part', name: 'core' }, noTests)
  const service = graph.cards.find(card => card.name === 'core/service.ts')!
  const db = graph.cards.find(card => card.name === 'core/db.ts')!
  assert.deepEqual(db.rows.map(row => row.id), [insert.id])
  assert.equal(db.folded, 1)
  assert.ok(service.rows.some(row => row.id === create.id) && !service.rows.some(row => row.id === check.id))
  const outside = graph.cards.find(card => card.id === 'part:api')!
  assert.deepEqual(outside.rows.map(row => [row.id, row.kind]), [['api/routes.ts', 'file']])
  assert.ok(graph.edges.some(edge => edge.from === 'api/routes.ts' && edge.to === create.id))
  // Shut by the viewer: the edge goes to the card's header.
  const flipped = focusGraph(index, { kind: 'part', name: 'core' }, { ...noTests, flipped: new Set(['part:api']) })
  assert.ok(flipped.edges.some(edge => edge.from === 'part:api' && edge.to === create.id))
})

test('calls between two rows of one card are kept apart from the lines between cards', () => {
  const a = fn('one.ts', 1, 'a')
  const b = fn('one.ts', 5, 'b')
  const c = fn('two.ts', 1, 'c')
  const list: ScannedFile[] = [
    { path: 'one.ts', part: 'p', test: false, lines: 10, functions: [a, b] },
    { path: 'two.ts', part: 'p', test: false, lines: 10, functions: [c] }
  ]
  const graph = focusGraph(indexOf(list, [{ from: a.id, to: b.id, count: 1 }, { from: a.id, to: c.id, count: 1 }]), { kind: 'function', id: a.id })
  assert.deepEqual(graph.inner.map(edge => [edge.from, edge.to]), [[a.id, b.id]])
  assert.deepEqual(graph.edges.map(edge => [edge.from, edge.to]), [[a.id, c.id]])
})

test('a part\'s files on no line are listed in one card', () => {
  const list: ScannedFile[] = [
    { path: 'p/a.ts', part: 'p', test: false, lines: 5, functions: [fn('p/a.ts', 1, 'a')] },
    { path: 'p/b.ts', part: 'p', test: false, lines: 5, functions: [fn('p/b.ts', 1, 'b')] },
    { path: 'p/c.ts', part: 'p', test: false, lines: 5, functions: [] }
  ]
  const graph = focusGraph(indexOf(list, []), { kind: 'part', name: 'p' })
  assert.deepEqual(graph.cards.map(card => [card.id, card.kind, card.rows.map(row => row.id)]), [['idle:p', 'idle', ['p/a.ts', 'p/b.ts', 'p/c.ts']]])
})

test('past the card limit the rest is counted per part in a more card', () => {
  const many: ScannedFile[] = []
  const edges: CallEdge[] = []
  const hub = fn('hub.ts', 1, 'hub')
  many.push({ path: 'hub.ts', part: 'core', test: false, lines: 5, functions: [hub] })
  for (let i = 0; i < 10; i++) {
    const caller = fn(`callers/c${i}.ts`, 1, `c${i}`)
    many.push({ path: `callers/c${i}.ts`, part: 'callers', test: false, lines: 5, functions: [caller] })
    edges.push({ from: caller.id, to: hub.id, count: i + 1 })
  }
  const graph = focusGraph(indexOf(many, edges), { kind: 'function', id: hub.id }, { maxCards: 4 })
  assert.equal(graph.cards.filter(card => card.kind === 'file').length, 4)
  // The heaviest callers are kept.
  assert.ok(graph.cards.some(card => card.name === 'callers/c9.ts'))
  assert.equal(graph.leftOut, 7)
  assert.deepEqual(graph.cards.filter(card => card.kind === 'more').map(card => [card.name, card.count]), [['callers', 7]])
})

test('past the row limit the lightest cards outside the focus are shut', () => {
  const graph = focusGraph(index, { kind: 'function', id: create.id }, { ...noTests, maxRows: 2 })
  assert.equal(graph.cards.find(card => card.home)!.shut, false)
  assert.ok(graph.cards.filter(card => !card.home).every(card => card.shut))
})

test('import edges join the cards, judged by the rules, type-only ones only when asked', () => {
  const between = [{ from: 'api', to: 'core', files: [
    { from: 'api/routes.ts', to: 'core/service.ts', typeOnly: false, breaks: [], known: false },
    { from: 'api/routes.ts', to: 'core/db.ts', typeOnly: true, breaks: ['layer' as const], known: false }
  ] }]
  const anchor = (path: string) => `file:${path}`
  const all = importEdges({ between, inner: null, typeOnly: true, rules: true }, anchor)
  assert.deepEqual(all.map(edge => [edge.to, edge.tone, edge.typeOnly]), [['file:core/service.ts', 'ok', false], ['file:core/db.ts', 'new', true]])
  assert.equal(importEdges({ between, inner: null, typeOnly: false, rules: true }, anchor).length, 1)
  assert.equal(importEdges({ between, inner: null, typeOnly: true, rules: true, onlyBreaks: true }, anchor).length, 1)
  const graph = focusGraph(index, { kind: 'file', path: 'api/routes.ts' }, { ...noTests, imports: { between, inner: null, typeOnly: true, rules: false } })
  assert.ok(graph.edges.some(edge => edge.kind === 'import' && edge.from === 'file:api/routes.ts' && edge.to === 'file:core/db.ts' && edge.tone === 'plain'))
})

test('a path view draws only the functions and edges given', () => {
  const graph = pathGraph(index, [handler.id, create.id, insert.id], [{ from: handler.id, to: create.id }], create.id, noTests)
  assert.deepEqual(graph.edges.map(edge => [edge.from, edge.to, edge.count]), [[handler.id, create.id, 2]])
  assert.equal(graph.cards.find(card => card.home)!.name, 'core/service.ts')
})

test('the project: parts as cards, files that import across parts as rows, shut when there are many', () => {
  const parts: Part[] = [{ name: 'api', layer: 'app', role: '', uses: [], folder: 'api' }, { name: 'core', layer: 'core', role: '', uses: [], folder: 'core' }]
  const between = [{ from: 'api', to: 'core', files: [{ from: 'api/routes.ts', to: 'core/service.ts', typeOnly: false, breaks: [], known: false }] }]
  const graph = projectGraph(parts, index.filesByPart, { between, inner: null, typeOnly: true, rules: true }, noTests)
  assert.deepEqual(graph.cards.map(card => [card.name, card.rows.map(row => row.id), card.folded]), [['api', ['api/routes.ts'], 0], ['core', ['core/service.ts'], 1]])
  assert.deepEqual(graph.edges.map(edge => [edge.from, edge.to, edge.tone]), [['api/routes.ts', 'core/service.ts', 'ok']])
  const shut = projectGraph(parts, index.filesByPart, { between, inner: null, typeOnly: true, rules: true }, { ...noTests, flipped: new Set(['part:core']) })
  assert.deepEqual(shut.edges.map(edge => [edge.from, edge.to]), [['api/routes.ts', 'part:core']])
})

test('assemble with no members still draws the home files', () => {
  const graph = assemble(index, { members: new Map(), home: [files[2]!] })
  assert.deepEqual(graph.cards.map(card => [card.name, card.rows.length, card.folded]), [['core/db.ts', 0, 2]])
})

test('a function\'s cards get columns: its own file in the middle, callers left, what it calls right', () => {
  const graph = focusGraph(index, { kind: 'function', id: create.id }, { ...noTests, depth: 2 })
  const column = (name: string) => graph.cards.find(card => card.name === name)!.column
  assert.equal(column('core/service.ts'), 0)
  assert.equal(column('api/routes.ts'), -1)
  assert.equal(column('core/db.ts'), 1)
})

test('the project\'s part cards get the column of their layer, in the layers\' order', () => {
  const parts: Part[] = [{ name: 'api', layer: 'app', role: '', uses: [], folder: 'api' }, { name: 'core', layer: 'core', role: '', uses: [], folder: 'core' }]
  const graph = projectGraph(parts, index.filesByPart, { between: [], inner: null, typeOnly: true, rules: false }, { ...noTests, layerOrder: ['app', 'core'] })
  assert.deepEqual(graph.cards.map(card => [card.name, card.column]), [['api', 0], ['core', 1]])
})
