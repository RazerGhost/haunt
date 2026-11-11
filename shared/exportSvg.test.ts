import assert from 'node:assert/strict'
import { test } from 'node:test'
import { escapeXml, graphSvg, type SvgColors } from './exportSvg.ts'

/* Export: the code graph as SVG, every text escaped. */

const colors: SvgColors = { background: 'rgb(1, 1, 1)', elevated: 'rgb(2, 2, 2)', border: 'rgb(3, 3, 3)', text: 'rgb(4, 4, 4)', muted: 'rgb(5, 5, 5)', layer: layer => (layer === 'kernel' ? 'rgb(10, 0, 0)' : 'rgb(0, 10, 0)') }

test('escapes text for XML', () => {
  assert.equal(escapeXml('a & b < c > d "e" \'f\''), 'a &amp; b &lt; c &gt; d &quot;e&quot; &apos;f&apos;')
  assert.equal(escapeXml('plain'), 'plain')
})

test('draws the code graph: the lines with arrowheads under the cards, every text escaped', () => {
  const svg = graphSvg([
    { x: 0, y: 0, w: 300, h: 100, title: 'a<b>.ts', folder: 'src/x', layer: 'kernel', dashed: false, rowsTop: 36, row: 26, rows: [{ label: 'load&save', letter: 'f', folder: '', depth: 0, context: false, notes: ['12'] }], footer: '2 more · Show' },
    { x: 420, y: 0, w: 300, h: 44, title: 'b.ts', folder: '', layer: null, dashed: true, rowsTop: 36, row: 26, rows: [], footer: '' }
  ], [{ curve: [300, 49, 360, 49, 360, 22, 420, 22], color: 'rgb(9, 9, 9)', width: 1.5, dashed: true, opacity: 0.85 }], { colors })
  assert.match(svg, /^<svg [^>]*viewBox="-24 -24 768 148"/)
  assert.ok(svg.indexOf('<path d="M300,49') < svg.indexOf('<rect x="0"'), 'lines are drawn before the cards')
  assert.match(svg, /stroke-dasharray="5 4"/)
  assert.match(svg, /a&lt;b&gt;.ts/)
  assert.match(svg, /load&amp;save/)
  assert.equal((svg.match(/<circle /g) ?? []).length, 1)
  // A bigger graph is drawn smaller.
  assert.match(graphSvg([{ x: 0, y: 0, w: 4000, h: 100, title: 'a', folder: '', layer: null, dashed: false, rowsTop: 36, row: 26, rows: [], footer: '' }], [], { colors, maxWidth: 1024 }), /^<svg [^>]*width="1024"/)
})
