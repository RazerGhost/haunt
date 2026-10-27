import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canvasTree, layout, type Box } from './canvas.ts'
import { canvasSvg, escapeXml, layersSvg, type SvgColors } from './exportSvg.ts'
import type { FunctionNode, Layer, Part, ScannedFile } from './types.ts'

/* Export: the canvas and the layer map as SVG, every box a rectangle, every text escaped. */

const fn = (name: string, line: number, children: FunctionNode[] = []): FunctionNode => ({ id: `${name}@${line}`, name, kind: 'function', line, endLine: line + 20, exported: true, async: false, anonymous: false, children })
const file = (path: string, part: string, functions: FunctionNode[]): ScannedFile => ({ path, part, test: false, lines: 80, functions })
const parts: Part[] = [{ name: 'core', folder: 'src', layer: 'kernel', role: '', uses: [] }, { name: 'web<&>', folder: 'web', layer: 'entry', role: '', uses: [] }]
const layers: Layer[] = [{ id: 'kernel', label: 'Kernel', note: '' }, { id: 'entry', label: 'Entry', note: '' }]
const files = [
  file('src/a.ts', 'core', [fn('load', 1, [fn('inner', 3)]), fn('save', 30)]),
  file('src/deep/b.ts', 'core', [fn('read', 1)]),
  file('web/"page".ts', 'web<&>', [fn('render<T>', 1)])
]
const colors: SvgColors = { background: 'rgb(1, 1, 1)', elevated: 'rgb(2, 2, 2)', border: 'rgb(3, 3, 3)', text: 'rgb(4, 4, 4)', muted: 'rgb(5, 5, 5)', layer: layer => (layer === 'kernel' ? 'rgb(10, 0, 0)' : 'rgb(0, 10, 0)') }

function tree(): Box {
  const root = canvasTree(files, parts, layers)
  layout(root, { x: 0, y: 0, w: 1000, h: 625 })
  return root
}
const count = (root: Box): number => root.children.reduce((sum, child) => sum + 1 + count(child), 0)

test('escapes text for XML', () => {
  assert.equal(escapeXml('a & b < c > d "e" \'f\''), 'a &amp; b &lt; c &gt; d &quot;e&quot; &apos;f&apos;')
  assert.equal(escapeXml('plain'), 'plain')
})

test('draws every box of the canvas as a rectangle, sized to the world', () => {
  const root = tree()
  const svg = canvasSvg(root, { width: 2000, colors })
  assert.match(svg, /^<svg xmlns="http:\/\/www.w3.org\/2000\/svg" width="2000" height="1250" viewBox="0 0 1000 625">/)
  // The background and one per box.
  assert.equal(svg.match(/<rect /g)?.length, 1 + count(root))
  assert.ok(svg.includes('fill="rgb(10, 0, 0)"'))
  assert.ok(svg.endsWith('</svg>'))
})

test('writes names escaped, and none in boxes too small for them', () => {
  const svg = canvasSvg(tree(), { width: 2000, colors })
  assert.ok(svg.includes('>web&lt;&amp;&gt;</text>'))
  assert.ok(svg.includes('&quot;page&quot;.ts'))
  assert.ok(!svg.includes('render<T>') && !svg.includes('web<&>'))
  const tiny = canvasSvg(tree(), { width: 20, colors })
  assert.equal(tiny.match(/<text /g), null)
})

test('draws the layer map as bands of tiles, escaped', () => {
  const svg = layersSvg([
    { layer: 'kernel', label: 'Kernel', size: '1 part · 4 fns', tiles: [{ name: 'core', size: '2 files · 4 fns' }] },
    { layer: 'entry', label: 'Entry & <more>', size: '1 part · 1 fns', tiles: [{ name: 'web<&>', size: '1 file · 1 fns' }] }
  ], { width: 800, colors })
  assert.match(svg, /^<svg xmlns="http:\/\/www.w3.org\/2000\/svg" width="800" height="(\d+)" viewBox="0 0 800 \1">/)
  // The background, two rectangles per band and two per tile.
  assert.equal(svg.match(/<rect /g)?.length, 1 + 2 * 2 + 2 * 2)
  assert.ok(svg.includes('ENTRY &amp; &lt;MORE&gt;'))
  assert.ok(svg.includes('>web&lt;&amp;&gt;</text>'))
  assert.ok(svg.includes('>2 files · 4 fns</text>'))
})

test('wraps a band\'s tiles into rows', () => {
  const tiles = Array.from({ length: 12 }, (_, at) => ({ name: `part-${at}`, size: '1 file · 1 fns' }))
  const one = layersSvg([{ layer: 'kernel', label: 'Kernel', size: '', tiles: tiles.slice(0, 1) }], { width: 800, colors })
  const many = layersSvg([{ layer: 'kernel', label: 'Kernel', size: '', tiles }], { width: 800, colors })
  const height = (svg: string) => Number(svg.match(/height="(\d+)"/)![1])
  assert.ok(height(many) > height(one))
})
