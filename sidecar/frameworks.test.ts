import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import { scriptView, viewName, viewSource } from './frameworks.ts'
import { scanner } from './scan.ts'
import type { FunctionNode, Scan } from '../shared/types.ts'

/* JavaScript, and Vue, Svelte and Astro files through their TypeScript views. */

const fixture = (name: string) => fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url))

function names(scan: Scan): Map<string, { name: string, path: string, line: number }> {
  const out = new Map<string, { name: string, path: string, line: number }>()
  for (const file of scan.files) {
    const walk = (items: FunctionNode[]) => items.forEach((item) => {
      out.set(item.id, { name: item.name, path: file.path, line: item.line })
      walk(item.children)
    })
    walk(file.functions)
  }
  return out
}

test('keeps only the script blocks, at the same lines and columns', () => {
  const svelte = '<script lang="ts">\n  let n = 1\n</script>\n\n<p>{n}</p>\n'
  const view = scriptView('A.svelte', svelte)
  assert.equal(view.text.length, svelte.length)
  assert.equal(view.text.split('\n').length, svelte.split('\n').length)
  assert.equal(view.text.split('\n')[1], '  let n = 1')
  assert.doesNotMatch(view.text, /<p>|<script/)
  assert.equal(view.tsx, false)

  const astro = '---\nconst a = 1\n---\n<h1>{a}</h1>\n<script>\nconst b = 2\n</script>\n'
  const astroView = scriptView('a.astro', astro).text.split('\n')
  assert.equal(astroView[1], 'const a = 1')
  assert.equal(astroView[5], 'const b = 2')
  assert.equal(astroView[0]!.trim(), '')

  assert.equal(scriptView('B.vue', '<script setup lang="tsx">\nconst x = <div/>\n</script>').tsx, true)
  assert.equal(scriptView('C.vue', '<script src="./c.ts"></script>').text.trim(), '')
  assert.equal(viewName('/p/A.svelte'), '/p/A.svelte.ts')
  assert.equal(viewSource('/p/B.vue.tsx'), '/p/B.vue')
  assert.equal(viewSource('/p/c.ts'), null)
})

test('scans JavaScript, Svelte, Vue and Astro files, with calls and imports between them', async () => {
  const scan = scanner(fixture('frameworks'))
  const structure = await scan.structure()
  // The minified file is left out; the rest of every kind is in.
  assert.deepEqual(structure.files.map(file => file.path), [
    'src/components/Card.vue',
    'src/legacy.js',
    'src/lib/Counter.svelte',
    'src/lib/format.ts',
    'src/pages/index.astro',
    'src/routes/+page.svelte'
  ])
  const found = names(structure)
  const where = (name: string) => [...found.values()].find(item => item.name === name)
  // Functions sit at their lines in the original files.
  assert.deepEqual(where('increment'), { name: 'increment', path: 'src/lib/Counter.svelte', line: 6 })
  assert.deepEqual(where('title'), { name: 'title', path: 'src/components/Card.vue', line: 10 })
  assert.deepEqual(where('label'), { name: 'label', path: 'src/pages/index.astro', line: 5 })
  assert.deepEqual(where('onLoad'), { name: 'onLoad', path: 'src/pages/index.astro', line: 12 })
  assert.deepEqual(where('helper'), { name: 'helper', path: 'src/legacy.js', line: 5 })

  const calls = scan.calls().calls.map(edge => `${found.get(edge.from)?.name} → ${found.get(edge.to)?.name}`).sort()
  assert.deepEqual(calls, ['increment → format', 'label → format', 'legacy → helper', 'load → legacy', 'title → format'])

  // Imports between parts: the page reaches Counter.svelte through the `$lib` alias, the Astro page the Vue file.
  const imports = structure.imports.map(group => `${group.from} → ${group.to}`).sort()
  assert.deepEqual(imports, ['components → lib', 'pages → components', 'pages → lib', 'routes → lib', 'routes → src'])
})
