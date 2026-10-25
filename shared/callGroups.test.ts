import assert from 'node:assert/strict'
import { test } from 'node:test'
import { GROUP_ABOVE, groupColumns, groupFolder } from './callGroups.ts'

/* The call graph's long columns, grouped by part, then by folder; opened groups show what is inside them. */

const where = (id: string) => {
  const [part, ...rest] = id.split('|')
  return { part: part!, partFolder: part === 'core' ? 'src' : 'apps/api', path: rest.join('|') }
}

test('names a function\'s group by the first two folders inside its part', () => {
  assert.equal(groupFolder({ part: 'core', partFolder: 'src', path: 'src/features/audit/server/audit.ts' }), 'features/audit/')
  assert.equal(groupFolder({ part: 'core', partFolder: 'src', path: 'src/kernel/errors.ts' }), 'kernel/')
  assert.equal(groupFolder({ part: 'core', partFolder: 'src', path: 'src/index.ts' }), './')
})

const center = 'core|src/kernel/x.ts'
function columns(): Map<string, { column: number, weight: number }> {
  const out = new Map<string, { column: number, weight: number }>()
  out.set(center, { column: 0, weight: 0 })
  // Seventeen callers: twelve in core's features/team, two in its features/audit, three in the api part.
  for (let i = 0; i < 12; i++) out.set(`core|src/features/team/server/t${i}.ts`, { column: -1, weight: 1 })
  out.set('core|src/features/audit/server/a.ts', { column: -1, weight: 3 })
  out.set('core|src/features/audit/server/b.ts', { column: -1, weight: 1 })
  for (let i = 0; i < 3; i++) out.set(`api|apps/api/src/routes/r${i}.ts`, { column: -1, weight: 2 })
  // Three callees: a short column.
  for (let i = 0; i < 3; i++) out.set(`core|src/kernel/c${i}.ts`, { column: 1, weight: 1 })
  return out
}
const describe = (shown: ReturnType<typeof groupColumns>['shown'], column: number) => shown
  .filter(item => item.column === column)
  .map(item => (item.kind === 'group' ? `${item.part}:${item.folder} ×${item.members.length}` : item.id))
  .sort()

test('a long column shows its parts, a short one its functions', () => {
  assert.ok(17 > GROUP_ABOVE)
  const { shown, shownAs } = groupColumns(columns(), where, new Set(), new Set([center]))
  assert.deepEqual(describe(shown, -1), ['api: ×3', 'core: ×14'])
  assert.equal(describe(shown, 1).length, 3)
  assert.equal(shownAs.get('core|src/features/team/server/t3.ts'), 'group:-1:core:')
})

test('an opened part shows its folders, an opened folder its functions', () => {
  const part = groupColumns(columns(), where, new Set(['core:']), new Set([center]))
  assert.deepEqual(describe(part.shown, -1), ['api: ×3', 'core:features/audit/ ×2', 'core:features/team/ ×12'])
  const folder = groupColumns(columns(), where, new Set(['core:', 'core:features/audit/']), new Set([center]))
  assert.deepEqual(describe(folder.shown, -1), ['api: ×3', 'core/features/audit/server/a.ts', 'core/features/audit/server/b.ts', 'core:features/team/ ×12'].map(item => item.replace('core/', 'core|src/')).sort())
})
