import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { FunctionNode, ScannedFile } from './types.ts'
import { unusedFunctions } from './unused.ts'

const fn = (id: string, name: string, extra: Partial<FunctionNode> = {}): FunctionNode =>
  ({ id, name, kind: 'function', line: Number(id.split(':')[1]), endLine: 1, exported: false, async: false, anonymous: false, children: [], ...extra })
const file = (path: string, functions: FunctionNode[], test = false): ScannedFile => ({ path, part: 'app', test, lines: 10, functions })

test('finds what nothing uses and exports used only in their own file, leaving out what others likely use', () => {
  const files = [
    file('src/lib.ts', [
      fn('src/lib.ts:1:1', 'used', { exported: true }),
      fn('src/lib.ts:2:1', 'onlyHere', { exported: true }),
      fn('src/lib.ts:3:1', 'nobody', { exported: true }),
      fn('src/lib.ts:4:1', 'dead'),
      fn('src/lib.ts:5:1', 'Box', { kind: 'class', children: [fn('src/lib.ts:6:1', 'run', { kind: 'method' })] }),
      fn('src/lib.ts:7:1', 'default', { exported: true }),
      fn('src/lib.ts:8:1', 'onSelect')
    ]),
    file('src/index.ts', [fn('src/index.ts:1:1', 'api', { exported: true }), fn('src/index.ts:2:1', 'helper')]),
    file('pages/home.vue', [fn('pages/home.vue:1:1', 'onClick')]),
    file('nuxt.config.ts', [fn('nuxt.config.ts:1:1', 'setup')]),
    file('src/lib.test.ts', [fn('src/lib.test.ts:1:1', 'helper')], true),
    file('src/lib.rs', [fn('src/lib.rs:1:1', 'rusty')])
  ]
  const uses = { 'src/lib.ts:1:1': 'outside', 'src/lib.ts:2:1': 'inside', 'src/lib.ts:5:1': 'inside', 'src/lib.ts:8:1': 'held' } as const
  const found = unusedFunctions(files, uses).map(item => `${item.kind}:${item.name}`)
  assert.deepEqual(found, ['dead:helper', 'export:onlyHere', 'unreferenced:nobody', 'dead:dead'])
})
