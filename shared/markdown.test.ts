import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type Block, type Inline, parseMarkdown, safeHref } from './markdown.ts'

/* A README's Markdown, read into a plain tree; nothing in it may become HTML or an unsafe link. */

const text = (value: string): Inline => ({ kind: 'text', text: value })
const paragraph = (...children: Inline[]): Block => ({ kind: 'paragraph', children })
const inline = (source: string): Inline[] => {
  const [block] = parseMarkdown(source)
  assert.equal(block?.kind, 'paragraph')
  return (block as Extract<Block, { kind: 'paragraph' }>).children
}

test('reads ATX and setext headings', () => {
  assert.deepEqual(parseMarkdown('# One\n## Two ##\n###### Six\n#Not a heading'), [
    { kind: 'heading', level: 1, children: [text('One')] },
    { kind: 'heading', level: 2, children: [text('Two')] },
    { kind: 'heading', level: 6, children: [text('Six')] },
    paragraph(text('#Not a heading'))
  ])
  assert.deepEqual(parseMarkdown('Title\n=====\n\nSub\n---'), [
    { kind: 'heading', level: 1, children: [text('Title')] },
    { kind: 'heading', level: 2, children: [text('Sub')] }
  ])
})

test('reads paragraphs, rules and quotes', () => {
  assert.deepEqual(parseMarkdown('one\ntwo\n\n***\n\n> quoted\nlazy\n> > deeper'), [
    paragraph(text('one two')),
    { kind: 'rule' },
    { kind: 'quote', blocks: [paragraph(text('quoted lazy')), { kind: 'quote', blocks: [paragraph(text('deeper'))] }] }
  ])
})

test('reads fenced and indented code as it is', () => {
  assert.deepEqual(parseMarkdown('```ts title\nconst a = `<b>` *not em*\n\n  indented\n```\n\n~~~\nx\n~~~\n\n    four spaces\n    more'), [
    { kind: 'code', lang: 'ts', text: 'const a = `<b>` *not em*\n\n  indented' },
    { kind: 'code', lang: '', text: 'x' },
    { kind: 'code', lang: '', text: 'four spaces\nmore' }
  ])
  // An unclosed fence runs to the end.
  assert.deepEqual(parseMarkdown('```\nopen'), [{ kind: 'code', lang: '', text: 'open' }])
})

test('reads lists: nested, ordered, tasks, tight and loose', () => {
  const [list] = parseMarkdown('- a\n- [x] done\n- [ ] todo\n  - nested\n    1. deep')
  assert.deepEqual(list, {
    kind: 'list', ordered: false, start: 1, tight: true, items: [
      { blocks: [paragraph(text('a'))] },
      { blocks: [paragraph(text('done'))], checked: true },
      { blocks: [paragraph(text('todo')), {
        kind: 'list', ordered: false, start: 1, tight: true, items: [
          { blocks: [paragraph(text('nested')), { kind: 'list', ordered: true, start: 1, tight: true, items: [{ blocks: [paragraph(text('deep'))] }] }] }
        ]
      }], checked: false }
    ]
  })
  const [ordered] = parseMarkdown('3. three\n\n4. four\n   still four')
  assert.deepEqual(ordered, {
    kind: 'list', ordered: true, start: 3, tight: false, items: [
      { blocks: [paragraph(text('three'))] },
      { blocks: [paragraph(text('four still four'))] }
    ]
  })
  // Another marker starts another list.
  assert.equal(parseMarkdown('- a\n+ b').length, 2)
})

test('reads pipe tables with their alignment', () => {
  assert.deepEqual(parseMarkdown('| Name | Size | Note |\n|:-----|-----:|:---:|\n| `a\\|b` | **2** |\n| c | 3 | x | extra'), [{
    kind: 'table',
    align: ['left', 'right', 'center'],
    header: [[text('Name')], [text('Size')], [text('Note')]],
    rows: [
      [[{ kind: 'code', text: 'a|b' }], [{ kind: 'strong', children: [text('2')] }], []],
      [[text('c')], [text('3')], [text('x')]]
    ]
  }])
  // A paragraph with a pipe but no delimiter row stays a paragraph.
  assert.equal(parseMarkdown('a | b\nc | d')[0]?.kind, 'paragraph')
})

test('reads emphasis, strike, code and breaks', () => {
  assert.deepEqual(inline('*em* _em_ **strong** __strong__ ***both*** ~~gone~~ `co*de*`'), [
    { kind: 'em', children: [text('em')] }, text(' '),
    { kind: 'em', children: [text('em')] }, text(' '),
    { kind: 'strong', children: [text('strong')] }, text(' '),
    { kind: 'strong', children: [text('strong')] }, text(' '),
    { kind: 'em', children: [{ kind: 'strong', children: [text('both')] }] }, text(' '),
    { kind: 'strike', children: [text('gone')] }, text(' '),
    { kind: 'code', text: 'co*de*' }
  ])
  assert.deepEqual(inline('snake_case_name and 2 * 3 * 4'), [text('snake_case_name and 2 * 3 * 4')])
  assert.deepEqual(inline('**a *b* c**'), [{ kind: 'strong', children: [text('a '), { kind: 'em', children: [text('b')] }, text(' c')] }])
  assert.deepEqual(inline('one  \ntwo\\\nthree\nfour'), [text('one'), { kind: 'break' }, text('two'), { kind: 'break' }, text('three four')])
  assert.deepEqual(inline('\\*not em\\* &amp; &lt;b&gt; &#65;&#x42; &nbsp;&bogus;'), [text('*not em* & <b> AB \u00A0&bogus;')])
})

test('reads links: inline, reference, autolinks and bare URLs', () => {
  assert.deepEqual(inline('[site](https://example.com "Title") [doc](docs/a.md) [top](#usage)'), [
    { kind: 'link', href: 'https://example.com', title: 'Title', children: [text('site')] }, text(' '),
    { kind: 'link', href: 'docs/a.md', children: [text('doc')] }, text(' '),
    { kind: 'link', href: '#usage', children: [text('top')] }
  ])
  assert.deepEqual(parseMarkdown('[full][Ref] [Ref][] [ref] [missing]\n\n[ref]: https://r.example \'R\''), [paragraph(
    { kind: 'link', href: 'https://r.example', title: 'R', children: [text('full')] }, text(' '),
    { kind: 'link', href: 'https://r.example', title: 'R', children: [text('Ref')] }, text(' '),
    { kind: 'link', href: 'https://r.example', title: 'R', children: [text('ref')] }, text(' [missing]')
  )])
  assert.deepEqual(inline('<https://a.example/x> <me@example.com> see https://b.example/p_(x). Done'), [
    { kind: 'link', href: 'https://a.example/x', children: [text('https://a.example/x')] }, text(' '),
    { kind: 'link', href: 'mailto:me@example.com', children: [text('me@example.com')] }, text(' see '),
    { kind: 'link', href: 'https://b.example/p_(x)', children: [text('https://b.example/p_(x)')] }, text('. Done')
  ])
  // A link in a link's text is only text.
  assert.deepEqual(inline('[a [b](c) d](e)')[0], { kind: 'link', href: 'e', children: [text('a [b](c) d')] })
})

test('unsafe links are their text only', () => {
  assert.deepEqual(inline('[click](javascript:alert(1)) [x](data:text/html,hi) [f](file:///etc/passwd) <javascript:alert(1)>'), [
    text('click x f javascript:alert(1)')
  ])
  assert.deepEqual(inline('[e](&#106;avascript:alert(1))'), [text('e')])
})

test('images are named, not loaded', () => {
  assert.deepEqual(inline('![The *logo*](logo.png) ![](x.svg) [![badge](https://ci/badge.svg)](https://ci)'), [
    { kind: 'image', src: 'logo.png', alt: 'The logo' }, text(' '),
    { kind: 'image', src: 'x.svg', alt: '' }, text(' '),
    { kind: 'link', href: 'https://ci', children: [{ kind: 'image', src: 'https://ci/badge.svg', alt: 'badge' }] }
  ])
})

test('HTML is kept as its text only', () => {
  assert.deepEqual(parseMarkdown('<p align="center">\n  <img src="logo.png" onerror="alert(1)">\n  <b>Haunt</b> &amp; co\n</p>\n\n<!-- a\ncomment -->\n\n<script>alert(1)</script>\n\nText <span onclick="x">kept</span><br>here <img src=x>'), [
    paragraph(text('Haunt & co')),
    paragraph(text('Text kept'), { kind: 'break' }, text('here '))
  ])
})

test('safeHref allows http(s), mailto and relative links only', () => {
  for (const ok of ['https://a.example', 'HTTP://a.example', 'mailto:me@example.com', 'docs/readme.md', './a', '../b', '#anchor', '/root', '?q=1']) {
    assert.ok(safeHref(ok), ok)
  }
  for (const bad of ['javascript:alert(1)', 'JaVaScRiPt:alert(1)', ' javascript:x', 'java\nscript:alert(1)', 'java\tscript:x', 'java script:x',
    '\u0000javascript:x', 'data:text/html,x', 'vbscript:x', 'file:///c:/x', 'C:\\Windows', '//evil.example', '', '   ']) {
    assert.equal(safeHref(bad), null, JSON.stringify(bad))
  }
  assert.equal(safeHref(' https://a.example/x '), 'https://a.example/x')
})

test('deep nesting does not throw', () => {
  const deep = (open: string, count: number) => open.repeat(count) + 'x'
  for (const source of [
    deep('> ', 5000),
    deep('*', 5000) + '*'.repeat(5000),
    deep('[', 5000) + ']'.repeat(5000),
    Array.from({ length: 2000 }, (_, at) => `${'  '.repeat(at)}- item`).join('\n'),
    deep('**a ', 3000) + ' b**'.repeat(3000),
    deep('![', 3000) + '](x)'.repeat(3000)
  ]) {
    assert.ok(Array.isArray(parseMarkdown(source)))
  }
})

test('a large or hostile input finishes quickly', () => {
  const readme = '# Title\n\nSome *text* with `code`, [a link](https://x.example) and **more**.\n\n- item\n  - nested\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```\ncode\n```\n\n'
  const sources = [
    readme.repeat(2000),
    '*a '.repeat(50_000),
    '_a '.repeat(50_000),
    '`'.repeat(10_000) + ' ' + '``x '.repeat(10_000),
    '[a](b '.repeat(30_000),
    '<a '.repeat(50_000),
    '<!--'.repeat(50_000),
    'https://a.example/' + ')'.repeat(100_000),
    '&#'.repeat(50_000),
    '> - '.repeat(10_000) + 'x',
    '[x]: y\n'.repeat(20_000) + '[x]'.repeat(20_000)
  ]
  for (const source of sources) {
    const started = performance.now()
    parseMarkdown(source)
    const took = performance.now() - started
    assert.ok(took < 2000, `${JSON.stringify(source.slice(0, 20))}…: ${Math.round(took)} ms`)
  }
})

test('never throws on odd input', () => {
  for (const source of ['', '\n\n\n', '\0', '\t- a\n\t\t- b', '|', '|-|', '- ', '1.', '#', '```', '[]()', '![](', '<', '&', '\\', '\r\n# a\r\n']) {
    assert.ok(Array.isArray(parseMarkdown(source)), JSON.stringify(source))
  }
})
