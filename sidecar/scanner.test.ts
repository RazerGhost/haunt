import assert from 'node:assert/strict'
import { test } from 'node:test'
import ts from 'typescript'
import { callGraph, callGraphInTurns } from './calls.ts'
import { fileFunctions } from './functions.ts'
import type { FunctionNode } from '../shared/types.ts'

/** A program over in-memory files, parsed the way the scanner parses them. */
function program(files: Record<string, string>) {
  const parsed = new Map(Object.entries(files).map(([name, text]) => [name, ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true)]))
  const options: ts.CompilerOptions = { noEmit: true, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, allowImportingTsExtensions: true, types: [], noLib: true }
  const host = ts.createCompilerHost(options)
  host.getSourceFile = name => parsed.get(name)
  host.fileExists = name => parsed.has(name)
  host.readFile = name => files[name]
  return { parsed, program: ts.createProgram({ rootNames: [...parsed.keys()], options, host }) }
}

const names = (items: FunctionNode[]): unknown[] => items.map(item => item.children.length ? [item.name, names(item.children)] : item.name)

test('nests functions where they are written and keeps only callbacks with named functions inside', () => {
  const { parsed } = program({
    '/a.ts': `
      export async function outer() {
        const inner = () => 1
        ;[1].map(n => n)
        ;[2].forEach(() => { function named() {} })
        return inner()
      }
      export class Box { constructor() {} get size() { return 1 } run() {} }
      export default () => 0
    `
  })
  const found = fileFunctions(parsed.get('/a.ts')!, 'src/a.ts')
  assert.deepEqual(names(found.tree), [
    ['outer', ['inner', ['→ [2].forEach()', ['named']]]],
    ['Box', ['constructor', 'size', 'run']],
    'default'
  ])
  const outer = found.tree[0]!
  assert.equal(outer.exported, true)
  assert.equal(outer.async, true)
  assert.equal(outer.kind, 'function')
  assert.equal(outer.children[1]!.anonymous, true)
  assert.equal(outer.id, 'src/a.ts:2:7')
  // outer, inner, the forEach callback, named, Box, constructor, size, run, default; not the map callback.
  assert.equal(found.byNode.size, 9)
})

test('names a test file\'s tests by their titles and counts their calls', () => {
  const { parsed, program: built } = program({
    '/lib.ts': `export function add(a: number, b: number) { return a + b }`,
    '/lib.test.ts': `
      import { add } from './lib.ts'
      declare const describe: any, it: any, test: any
      describe('add', () => {
        it('adds two numbers', () => { add(1, 2) })
        it.skip(\`adds \${'more'}\`, async () => { add(1, 2) })
      })
      test.each([1])('row %s', () => { add(1, 1) })
      ;[1].map(() => add(0, 0))
    `
  })
  const found = fileFunctions(parsed.get('/lib.test.ts')!, 'lib.test.ts', true)
  assert.deepEqual(names(found.tree), [["→ describe()", ['adds two numbers', "adds ${'more'}"]], 'row %s'])
  assert.equal(found.tree[1]!.test, true)
  assert.equal(found.tree[0]!.test, undefined)
  // Not in a test file: the same callbacks stay unnamed (and dropped).
  assert.deepEqual(names(fileFunctions(parsed.get('/lib.test.ts')!, 'lib.test.ts').tree), [])
  const functions = new Map([...parsed].map(([name, sf]) => [name, fileFunctions(sf, name.slice(1), name.endsWith('.test.ts')).byNode]))
  const edges = callGraph(built, functions).calls.filter(edge => edge.to === 'lib.ts:1:1').map(edge => edge.from).sort()
  assert.deepEqual(edges, ['lib.test.ts:5:32', 'lib.test.ts:6:35', 'lib.test.ts:8:32'])
})

test('notes how each function is used: called or passed, from another file or only its own', () => {
  const { parsed, program: built } = program({
    '/lib.ts': `
      export function called() {}
      export function passed() {}
      export function onlyHere() {}
      export function nobody() {}
      function dead() { dead() }
      export function wraps() { onlyHere() }
      export const menu = { onSelect: () => called() }
      function shorthand() {}
      export const host = { shorthand }
      export function maker() { return function made() {} }
      export function facing() {}
    `,
    '/star.ts': `
      export function starred() {}
    `,
    '/main.ts': `
      import { called, passed, nobody } from './lib.ts'
      export { nobody as renamedButUnused }
      export { facing } from './lib.ts'
      export * from './star.ts'
      export function main() { called(); [1].map(passed) }
    `
  })
  const functions = new Map([...parsed].map(([name, sf]) => [name, fileFunctions(sf, name.slice(1)).byNode]))
  const { uses } = callGraph(built, functions)
  const byName = new Map([...functions.values()].flatMap(nodes => [...nodes.values()]).map(item => [item.name, uses[item.id]]))
  assert.equal(byName.get('called'), 'outside')
  assert.equal(byName.get('passed'), 'outside', 'passed as a value counts')
  assert.equal(byName.get('onlyHere'), 'inside', 'an export used only in its own file')
  assert.equal(byName.get('nobody'), 'outside', 'exported on from another file: part of that file\'s face')
  assert.equal(byName.get('facing'), 'outside', 're-exported from another file')
  assert.equal(byName.get('shorthand'), 'inside', 'put in an object by shorthand')
  assert.equal(byName.get('made'), 'held', 'returned as a value')
  assert.equal(byName.get('starred'), 'outside', 'exported on by export *')
  assert.equal(byName.get('dead'), undefined, 'a function calling itself uses nothing')
  assert.equal(byName.get('onSelect'), 'held', 'written into an object: whoever gets the object calls it')
})

test('resolves calls across files, through imports, methods and new, and counts call sites', () => {
  const { parsed, program: built } = program({
    '/lib.ts': `
      export function helper() {}
      export const arrow = () => helper()
      export class Service { run() { this.step(); this.step() } step() { helper() } }
    `,
    '/main.ts': `
      import { helper, arrow as renamed, Service } from './lib.ts'
      export function main() {
        helper()
        renamed()
        new Service().run()
        ;[1].forEach(() => helper())
      }
    `
  })
  const functions = new Map([...parsed].map(([name, sf]) => [name, fileFunctions(sf, name.slice(1)).byNode]))
  const nameOf = new Map([...functions.values()].flatMap(nodes => [...nodes.values()]).map(item => [item.id, item.name]))
  const edges = callGraph(built, functions).calls.map(edge => `${nameOf.get(edge.from)} → ${nameOf.get(edge.to)} ×${edge.count}`).sort()
  assert.deepEqual(edges, [
    'arrow → helper ×1',
    'main → Service ×1', // new Service()
    'main → arrow ×1', // imported under another name
    'main → helper ×2', // once directly, once inside a callback
    'main → run ×1',
    'run → step ×2',
    'step → helper ×1'
  ])
})

test('works the call graph out in turns, the same as at once, and stops when a newer scan asks', async () => {
  const files: Record<string, string> = {}
  for (let at = 0; at < 200; at++) files[`/f${at}.ts`] = `import { f${(at + 1) % 200} } from './f${(at + 1) % 200}.ts'
export function f${at}() { return f${(at + 1) % 200}() }
`
  const { parsed, program: built } = program(files)
  const functions = new Map([...parsed].map(([name, sf]) => [name, fileFunctions(sf, name.slice(1)).byNode]))
  const atOnce = callGraph(built, functions)
  const inTurns = await callGraphInTurns(built, functions, () => false)
  assert.deepEqual(inTurns, atOnce)
  assert.equal(atOnce.calls.length, 200)
  // Asked to stop at its first turn: no graph.
  let turns = 0
  const slow = { ...performance, now: () => (turns += 50) }
  const real = globalThis.performance
  Object.defineProperty(globalThis, 'performance', { value: slow, configurable: true })
  try {
    assert.equal(await callGraphInTurns(built, functions, () => true), null)
  } finally {
    Object.defineProperty(globalThis, 'performance', { value: real, configurable: true })
  }
})
