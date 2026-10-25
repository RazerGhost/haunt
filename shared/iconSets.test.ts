import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import iconSets from './iconSets.json' with { type: 'json' }

const root = join(import.meta.dirname, '..')
const PREFIX: Record<string, string> = { bootstrap: 'bi', phosphor: 'ph', tabler: 'tabler', remix: 'ri', material: 'material-symbols', heroicons: 'heroicons', iconoir: 'iconoir' }

function files(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) files(path, out)
    else if (/\.(vue|ts)$/.test(name)) out.push(path)
  }
  return out
}

test('every Lucide icon the window names is in the icon map (run scripts/icon-sets.mjs after adding one)', () => {
  const named = new Set(iconSets.named)
  const used = files(join(root, 'app')).flatMap(file => [...readFileSync(file, 'utf8').matchAll(/i-lucide-([a-z0-9-]+)/g)].map(match => match[1]!))
  assert.deepEqual([...new Set(used)].filter(name => !named.has(name)), [])
})

test('every mapped icon is in its set', () => {
  for (const [set, map] of Object.entries(iconSets.sets as Record<string, Record<string, string>>)) {
    const icons = JSON.parse(readFileSync(join(root, 'node_modules', '@iconify-json', PREFIX[set]!, 'icons.json'), 'utf8'))
    for (const target of Object.values(map)) {
      const [prefix, name] = target.split(':')
      assert.equal(prefix, PREFIX[set])
      assert.ok(icons.icons[name!] || icons.aliases?.[name!], `${target} is not in ${set}`)
    }
  }
})
