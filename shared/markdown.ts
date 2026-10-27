/*
 * Markdown, read into a plain tree, for showing a repository's README. The text comes from any repository on disk, and
 * the window can call Haunt's commands, so nothing here makes HTML: MarkdownView renders the tree as elements and text.
 * Raw HTML is kept as its text only, images are only named, and a link's href is kept only when safeHref allows it.
 *
 * CommonMark-ish, close enough for READMEs: headings, paragraphs, code (fenced and indented), quotes, lists (nested,
 * ordered, tasks), rules, GFM pipe tables; inline code, emphasis, strike, links (inline, reference, autolinks and bare
 * https:// URLs), images, hard breaks, escapes and the common entities. It never throws, nests at most MAX_DEPTH deep,
 * and each pass over the text is linear, so hostile input stays cheap.
 */

export type Inline =
  | { kind: 'text', text: string }
  | { kind: 'code', text: string }
  | { kind: 'strong' | 'em' | 'strike', children: Inline[] }
  | { kind: 'link', href: string, title?: string, children: Inline[] }
  | { kind: 'image', src: string, alt: string }
  | { kind: 'break' }

export type Align = 'left' | 'center' | 'right' | null

export interface ListItem { blocks: Block[], checked?: boolean }

export type Block =
  | { kind: 'heading', level: 1 | 2 | 3 | 4 | 5 | 6, children: Inline[] }
  | { kind: 'paragraph', children: Inline[] }
  | { kind: 'code', lang: string, text: string }
  | { kind: 'quote', blocks: Block[] }
  | { kind: 'list', ordered: boolean, start: number, tight: boolean, items: ListItem[] }
  | { kind: 'table', align: Align[], header: Inline[][], rows: Inline[][][] }
  | { kind: 'rule' }

export const MAX_DEPTH = 20

/** The href when it is http(s), mailto or relative (a path or an anchor); null for anything else. */
export function safeHref(href: string): string | null {
  // eslint-disable-next-line no-control-regex
  const cleaned = href.replace(/[\u0000-\u001F\u007F]/g, '').trim()
  if (!cleaned) return null
  const scheme = cleaned.match(/^([^/?#]*?):/)
  if (scheme) {
    const name = scheme[1]!.replace(/\s+/g, '').toLowerCase()
    return name === 'http' || name === 'https' || name === 'mailto' ? cleaned : null
  }
  // `//host` and `\\host` lead to another host without a scheme.
  if (/^[/\\]{2}/.test(cleaned)) return null
  return cleaned
}

export function parseMarkdown(text: string): Block[] {
  try {
    const lines = String(text).replace(/\r\n?/g, '\n').replace(/\0/g, '�').split('\n').map(expandTabs)
    const refs = new Map<string, { href: string, title?: string }>()
    const raw = blocks(lines, 0, refs)
    return raw.map(block => finish(block, refs, 0))
  } catch {
    return [{ kind: 'paragraph', children: [{ kind: 'text', text: String(text) }] }]
  }
}

/* Blocks: first the structure, with the inline text still raw; then the inline parts, once every reference is known. */

type Raw =
  | { kind: 'heading', level: 1 | 2 | 3 | 4 | 5 | 6, text: string }
  | { kind: 'paragraph', text: string, plain?: boolean }
  | { kind: 'code', lang: string, text: string }
  | { kind: 'quote', blocks: Raw[] }
  | { kind: 'list', ordered: boolean, start: number, tight: boolean, items: { blocks: Raw[], checked?: boolean }[] }
  | { kind: 'table', align: Align[], header: string[], rows: string[][] }
  | { kind: 'rule' }

type Refs = Map<string, { href: string, title?: string }>

function expandTabs(line: string): string {
  if (!line.includes('\t')) return line
  let out = ''
  for (const char of line) out += char === '\t' ? ' '.repeat(4 - (out.length % 4)) : char
  return out
}

const BLANK = /^\s*$/
const FENCE = /^( {0,3})(`{3,}|~{3,})(.*)$/
const ATX = /^ {0,3}(#{1,6})(?=[ \t]|$)(.*)$/
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/
const SETEXT = /^ {0,3}(=+|-+)[ \t]*$/
const QUOTE = /^ {0,3}> ?/
const ITEM = /^( {0,3})([-+*]|\d{1,9}[.)])( +|$)(.*)$/
const HTML = /^ {0,3}<(?:!--|[!?]|\/?[a-zA-Z][\w-]*(?:[\s/>]|$))/
// Only block-level tags end a paragraph; a line starting with <code> or <img> carries it on.
const HTML_INTERRUPTS = /^ {0,3}<(?:!--|\/?(?:address|article|aside|blockquote|center|details|dialog|div|dl|figure|footer|form|h[1-6]|header|hr|li|nav|ol|p|picture|pre|script|section|style|summary|table|ul)(?:[\s/>]|$))/i
const DELIMITER_ROW = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/
const DEFINITION = /^ {0,3}\[((?:[^\]\\]|\\.){1,999})\]:[ \t]*(<[^<>\n]*>|\S+)(?:[ \t]+("[^"]*"|'[^']*'|\([^()]*\)))?[ \t]*$/

const indentOf = (line: string) => line.length - line.trimStart().length

function fenceOpen(line: string): { indent: number, char: string, length: number, info: string } | null {
  const found = line.match(FENCE)
  if (!found) return null
  const [, indent, fence, info] = found
  if (fence![0] === '`' && info!.includes('`')) return null
  return { indent: indent!.length, char: fence![0]!, length: fence!.length, info: info!.trim() }
}

/** Whether a line starts a block that ends a paragraph. */
function interrupts(line: string): boolean {
  if (fenceOpen(line) || ATX.test(line) || RULE.test(line) || QUOTE.test(line) || HTML_INTERRUPTS.test(line)) return true
  const item = line.match(ITEM)
  if (!item || BLANK.test(item[4]!)) return false
  return !/\d/.test(item[2]![0]!) || (item[2]!.startsWith('1') && item[2]!.length === 2)
}

function splitRow(line: string): string[] {
  let row = line.trim()
  if (row.startsWith('|')) row = row.slice(1)
  if (row.endsWith('|') && !row.endsWith('\\|')) row = row.slice(0, -1)
  const cells: string[] = []
  let cell = ''
  for (let i = 0; i < row.length; i++) {
    if (row[i] === '\\' && row[i + 1] === '|') {
      cell += '|'
      i++
    } else if (row[i] === '|') {
      cells.push(cell.trim())
      cell = ''
    } else {
      cell += row[i]
    }
  }
  cells.push(cell.trim())
  return cells
}

/** Whether a pipe table starts at line `at`: a header row, then a delimiter row with as many cells. */
function tableStarts(lines: string[], at: number): boolean {
  const header = lines[at]!
  const delimiter = lines[at + 1]
  if (!header.includes('|') || delimiter === undefined || !delimiter.includes('|') || !DELIMITER_ROW.test(delimiter)) return false
  return splitRow(header).length === splitRow(delimiter).length
}

function normalizeLabel(label: string): string {
  return label.trim().replace(/\s+/g, ' ').toLowerCase()
}

/** An HTML block as its text: tags, comments and scripts dropped, entities read. */
export function htmlText(html: string): string {
  return decodeEntities(html
    .replace(/<!--[\s\S]*?(?:-->|$)/g, '')
    .replace(/<(script|style)\b[^<>]*>[\s\S]*?(?:<\/\1\s*>|$)/gi, '')
    .replace(/<\/?([a-zA-Z][\w-]*)[^<>]*>/g, (_, name: string) => (/^(br|p|div|li|tr|td|th|h[1-6]|img|hr|table|ul|ol|summary|details)$/i.test(name) ? ' ' : '')))
    .replace(/\s+/g, ' ')
    .trim()
}

function blocks(lines: string[], depth: number, refs: Refs): Raw[] {
  const out: Raw[] = []
  if (depth >= MAX_DEPTH) {
    const text = lines.map(line => line.trim()).filter(Boolean).join(' ')
    return text ? [{ kind: 'paragraph', text, plain: true }] : []
  }
  let i = 0
  while (i < lines.length) {
    const line = lines[i]!
    if (BLANK.test(line)) {
      i++
      continue
    }

    const fence = fenceOpen(line)
    if (fence) {
      const body: string[] = []
      i++
      while (i < lines.length) {
        const close = lines[i]!.match(/^ {0,3}(`{3,}|~{3,})[ \t]*$/)
        if (close && close[1]![0] === fence.char && close[1]!.length >= fence.length) {
          i++
          break
        }
        const current = lines[i]!
        body.push(current.slice(Math.min(fence.indent, indentOf(current))))
        i++
      }
      out.push({ kind: 'code', lang: unescape(decodeEntities(fence.info.split(/\s+/)[0] ?? '')), text: body.join('\n') })
      continue
    }

    const atx = line.match(ATX)
    if (atx) {
      const text = atx[2]!.trim().replace(/(?:^|[ \t]+)#+[ \t]*$/, '')
      out.push({ kind: 'heading', level: atx[1]!.length as 1, text })
      i++
      continue
    }

    if (RULE.test(line)) {
      out.push({ kind: 'rule' })
      i++
      continue
    }

    if (indentOf(line) >= 4) {
      const body: string[] = []
      while (i < lines.length && (BLANK.test(lines[i]!) || indentOf(lines[i]!) >= 4)) body.push(lines[i++]!.slice(4))
      while (body.length && BLANK.test(body[body.length - 1]!)) body.pop()
      out.push({ kind: 'code', lang: '', text: body.join('\n') })
      continue
    }

    if (QUOTE.test(line)) {
      const inner: string[] = []
      while (i < lines.length) {
        const current = lines[i]!
        if (QUOTE.test(current)) inner.push(current.replace(QUOTE, ''))
        // A lazy line carries on the quote's paragraph.
        else if (!BLANK.test(current) && inner.length && !BLANK.test(inner[inner.length - 1]!) && !interrupts(current)) inner.push(current)
        else break
        i++
      }
      out.push({ kind: 'quote', blocks: blocks(inner, depth + 1, refs) })
      continue
    }

    if (ITEM.test(line)) {
      i = list(lines, i, depth, refs, out)
      continue
    }

    if (HTML.test(line)) {
      const comment = /^ {0,3}<!--/.test(line)
      const body: string[] = []
      while (i < lines.length) {
        const current = lines[i]!
        if (!comment && BLANK.test(current)) break
        body.push(current)
        i++
        if (comment && current.includes('-->')) break
      }
      const text = htmlText(body.join('\n'))
      if (text) out.push({ kind: 'paragraph', text, plain: true })
      continue
    }

    if (tableStarts(lines, i)) {
      const header = splitRow(line)
      const align = splitRow(lines[i + 1]!).map((cell): Align => {
        const left = cell.startsWith(':')
        const right = cell.endsWith(':')
        return left && right ? 'center' : right ? 'right' : left ? 'left' : null
      })
      i += 2
      const rows: string[][] = []
      while (i < lines.length && !BLANK.test(lines[i]!) && !interrupts(lines[i]!)) {
        const cells = splitRow(lines[i]!)
        rows.push(header.map((_, column) => cells[column] ?? ''))
        i++
      }
      out.push({ kind: 'table', align, header, rows })
      continue
    }

    // A paragraph, or a setext heading when underlined.
    const body: string[] = [line.trimStart()]
    i++
    let heading: 1 | 2 | 0 = 0
    while (i < lines.length) {
      const current = lines[i]!
      if (BLANK.test(current)) break
      const underline = current.match(SETEXT)
      if (underline) {
        heading = underline[1]![0] === '=' ? 1 : 2
        i++
        break
      }
      if (interrupts(current) || tableStarts(lines, i)) break
      body.push(current.trimStart())
      i++
    }
    // Reference definitions at the start of a paragraph are kept, not shown.
    while (body.length) {
      const found = body[0]!.match(DEFINITION)
      if (!found) break
      const label = normalizeLabel(found[1]!)
      let href = found[2]!
      if (href.startsWith('<')) href = href.slice(1, -1)
      if (label && !refs.has(label)) {
        const title = found[3] ? unescape(decodeEntities(found[3].slice(1, -1))) : undefined
        refs.set(label, { href: unescape(decodeEntities(href)), ...(title !== undefined ? { title } : {}) })
      }
      body.shift()
    }
    if (!body.length) continue
    const text = body.join('\n').trimEnd()
    if (heading) out.push({ kind: 'heading', level: heading, text: text.trim() })
    else out.push({ kind: 'paragraph', text })
  }
  return out
}

/** A list from line `start`: its items, each its own blocks. Returns the line after it. */
function list(lines: string[], start: number, depth: number, refs: Refs, out: Raw[]): number {
  const first = lines[start]!.match(ITEM)!
  const ordered = /\d/.test(first[2]![0]!)
  const marker = ordered ? first[2]!.slice(-1) : first[2]!
  const items: { blocks: Raw[], checked?: boolean }[] = []
  let tight = true
  let blankBefore = false
  let i = start
  while (i < lines.length) {
    const found = lines[i]!.match(ITEM)
    if (!found) break
    const isOrdered = /\d/.test(found[2]![0]!)
    if (isOrdered !== ordered || (ordered ? found[2]!.slice(-1) : found[2]!) !== marker) break
    if (items.length && RULE.test(lines[i]!)) break
    // A blank line between items, or between an item's own blocks, makes the list loose.
    if (blankBefore) tight = false
    const spaces = found[3]!.length
    const width = found[1]!.length + found[2]!.length + (spaces === 0 ? 1 : spaces > 4 ? 1 : spaces)
    let content = spaces > 4 ? ' '.repeat(spaces - 1) + found[4]! : found[4]!
    let checked: boolean | undefined
    const task = content.match(/^\[([ xX])\](?:[ \t]+|$)/)
    if (task) {
      checked = task[1] !== ' '
      content = content.slice(task[0].length)
    }
    const inner: string[] = [content]
    i++
    while (i < lines.length) {
      const current = lines[i]!
      if (BLANK.test(current)) {
        inner.push('')
        i++
        continue
      }
      if (indentOf(current) >= width) {
        inner.push(current.slice(width))
        i++
        continue
      }
      // A lazy line carries on the item's paragraph.
      const previous = inner[inner.length - 1]!
      if (!BLANK.test(previous) && !interrupts(current) && !ITEM.test(current) && !SETEXT.test(current)) {
        inner.push(current.trim())
        i++
        continue
      }
      break
    }
    blankBefore = false
    while (inner.length > 1 && BLANK.test(inner[inner.length - 1]!)) {
      inner.pop()
      blankBefore = true
    }
    if (inner.some((row, at) => {
      const next = inner[at + 1] ?? ''
      return at > 0 && BLANK.test(row) && !BLANK.test(next) && indentOf(next) === 0 && !ITEM.test(next)
    })) tight = false
    items.push({ blocks: blocks(inner, depth + 1, refs), ...(checked !== undefined ? { checked } : {}) })
  }
  const startAt = ordered ? Math.min(Number.parseInt(first[2]!, 10), 999_999_999) : 1
  out.push({ kind: 'list', ordered, start: startAt, tight, items })
  return i
}

function finish(block: Raw, refs: Refs, depth: number): Block {
  switch (block.kind) {
    case 'heading': return { kind: 'heading', level: block.level, children: parseInline(block.text, refs) }
    case 'paragraph': return { kind: 'paragraph', children: block.plain ? [{ kind: 'text', text: block.text }] : parseInline(block.text, refs) }
    case 'code': return block
    case 'rule': return block
    case 'quote': return { kind: 'quote', blocks: block.blocks.map(inner => finish(inner, refs, depth + 1)) }
    case 'list': return {
      kind: 'list',
      ordered: block.ordered,
      start: block.start,
      tight: block.tight,
      items: block.items.map(item => ({ blocks: item.blocks.map(inner => finish(inner, refs, depth + 1)), ...(item.checked !== undefined ? { checked: item.checked } : {}) }))
    }
    case 'table': return {
      kind: 'table',
      align: block.align,
      header: block.header.map(cell => parseInline(cell, refs)),
      rows: block.rows.map(row => row.map(cell => parseInline(cell, refs)))
    }
  }
}

/* Inline */

const ENTITIES: Record<string, string> = {
  amp: '&', lt: '<', gt: '>', quot: '"', apos: '\'', nbsp: ' ', copy: '©', reg: '®', trade: '™', hellip: '…',
  mdash: '—', ndash: '–', laquo: '«', raquo: '»', middot: '·', bull: '•', times: '×', larr: '←', rarr: '→', check: '✓'
}

function entity(name: string): string | null {
  if (name[0] === '#') {
    const code = name[1] === 'x' || name[1] === 'X' ? Number.parseInt(name.slice(2), 16) : Number.parseInt(name.slice(1), 10)
    if (!Number.isFinite(code)) return null
    return code === 0 || code > 0x10FFFF || (code >= 0xD800 && code <= 0xDFFF) ? '�' : String.fromCodePoint(code)
  }
  return ENTITIES[name] ?? null
}

export function decodeEntities(text: string): string {
  return text.replace(/&(#\d{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z][a-zA-Z0-9]{1,31});/g, (whole, name: string) => entity(name) ?? whole)
}

const PUNCTUATION = /[!-/:-@[-`{-~\p{P}\p{S}]/u
const unescape = (text: string) => text.replace(/\\([!-/:-@[-`{-~])/g, '$1')
const isSpace = (char: string | undefined) => char === undefined || /\s/.test(char)
const isPunct = (char: string | undefined) => char !== undefined && PUNCTUATION.test(char)

function leftFlanking(src: string, at: number, length: number): boolean {
  const before = src[at - 1]
  const after = src[at + length]
  return !isSpace(after) && (!isPunct(after) || isSpace(before) || isPunct(before))
}
function rightFlanking(src: string, at: number, length: number): boolean {
  const before = src[at - 1]
  const after = src[at + length]
  return !isSpace(before) && (!isPunct(before) || isSpace(after) || isPunct(after))
}
function canOpen(src: string, at: number, length: number): boolean {
  if (!leftFlanking(src, at, length)) return false
  return src[at] !== '_' || !rightFlanking(src, at, length) || isPunct(src[at - 1])
}
function canClose(src: string, at: number, length: number): boolean {
  if (!rightFlanking(src, at, length)) return false
  return src[at] !== '_' || !leftFlanking(src, at, length) || isPunct(src[at + length])
}

const runLength = (src: string, at: number) => {
  let end = at
  while (src[end] === src[at]) end++
  return end - at
}

const AUTOLINK = /<([a-zA-Z][a-zA-Z0-9+.-]{1,31}:[^\s<>]*)>/y
const EMAIL = /<([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)>/y
const TAG = /<\/?([a-zA-Z][\w-]*)(?:\s+[^<>]*)?\/?>/y
const ENTITY = /&(#\d{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z][a-zA-Z0-9]{1,31});/y
const BARE_URL = /https?:\/\/[^\s<>]+/y
const DESTINATION = /\(\s*(<[^<>\n]{0,2000}>|[^\s()]{0,2000}(?:\([^\s()]{0,2000}\)[^\s()]{0,2000})*)(?:\s+("[^"]{0,1000}"|'[^']{0,1000}'|\([^()]{0,1000}\)))?\s*\)/y

const sticky = (pattern: RegExp, src: string, at: number) => {
  pattern.lastIndex = at
  return pattern.exec(src)
}

/** The inline parts of a block's text. */
export function parseInline(src: string, refs: Refs = new Map(), depth = 0, inLink = false): Inline[] {
  if (depth >= MAX_DEPTH) return src ? [{ kind: 'text', text: src }] : []
  const out: Inline[] = []
  let text = ''
  const push = (node: Inline) => {
    flush()
    const last = out[out.length - 1]
    if (node.kind === 'text' && last?.kind === 'text') last.text += node.text
    else out.push(node)
  }
  const flush = () => {
    const last = out[out.length - 1]
    if (text && last?.kind === 'text') last.text += text
    else if (text) out.push({ kind: 'text', text })
    text = ''
  }
  // Searches that found nothing, so the same search later (from further on) is not run again.
  const noCodeClose = new Set<number>()
  const noClose = new Map<string, number>()
  const brackets = matchBrackets(src)

  let i = 0
  while (i < src.length) {
    const char = src[i]!

    if (char === '\\') {
      const next = src[i + 1]
      if (next === '\n') {
        push({ kind: 'break' })
        i += 2
        continue
      }
      if (next !== undefined && /[!-/:-@[-`{-~]/.test(next)) {
        text += next
        i += 2
        continue
      }
      text += char
      i++
      continue
    }

    if (char === '\n') {
      const hard = / {2,}$/.test(text)
      text = text.replace(/ +$/, '')
      if (hard) push({ kind: 'break' })
      else text += ' '
      i++
      while (src[i] === ' ') i++
      continue
    }

    if (char === '`') {
      const length = runLength(src, i)
      let close = -1
      if (!noCodeClose.has(length)) {
        let from = i + length
        while (from < src.length) {
          const at = src.indexOf('`', from)
          if (at < 0) break
          const run = runLength(src, at)
          if (run === length) {
            close = at
            break
          }
          from = at + run
        }
        if (close < 0) noCodeClose.add(length)
      }
      if (close < 0) {
        text += src.slice(i, i + length)
        i += length
        continue
      }
      let code = src.slice(i + length, close).replace(/\n/g, ' ')
      if (code.length > 2 && code.startsWith(' ') && code.endsWith(' ') && code.trim()) code = code.slice(1, -1)
      push({ kind: 'code', text: code })
      i = close + length
      continue
    }

    if (char === '<') {
      const auto = sticky(AUTOLINK, src, i)
      if (auto) {
        const href = safeHref(auto[1]!)
        if (href && !inLink) push({ kind: 'link', href, children: [{ kind: 'text', text: auto[1]! }] })
        else text += auto[1]
        i += auto[0].length
        continue
      }
      const email = sticky(EMAIL, src, i)
      if (email) {
        if (!inLink) push({ kind: 'link', href: `mailto:${email[1]}`, children: [{ kind: 'text', text: email[1]! }] })
        else text += email[1]
        i += email[0].length
        continue
      }
      if (src.startsWith('<!--', i)) {
        const end = src.indexOf('-->', i + 4)
        i = end < 0 ? src.length : end + 3
        continue
      }
      const tag = sticky(TAG, src, i)
      if (tag) {
        // Inline HTML is dropped; its text stays. A <br> still breaks the line.
        if (tag[1]!.toLowerCase() === 'br') push({ kind: 'break' })
        i += tag[0].length
        continue
      }
      text += char
      i++
      continue
    }

    if (char === '&') {
      const found = sticky(ENTITY, src, i)
      const decoded = found ? entity(found[1]!) : null
      if (found && decoded !== null) {
        text += decoded
        i += found[0].length
        continue
      }
      text += char
      i++
      continue
    }

    if (char === '[' || (char === '!' && src[i + 1] === '[')) {
      const image = char === '!'
      const open = image ? i + 1 : i
      const close = brackets.get(open)
      if (close !== undefined && (image || !inLink)) {
        const label = src.slice(open + 1, close)
        let target: { href: string, title?: string } | null = null
        let end = close + 1
        const inline = src[close + 1] === '(' ? sticky(DESTINATION, src, close + 1) : null
        if (inline) {
          let href = inline[1]!
          if (href.startsWith('<')) href = href.slice(1, -1)
          target = { href: unescape(decodeEntities(href)), ...(inline[2] ? { title: unescape(decodeEntities(inline[2].slice(1, -1))) } : {}) }
          end = close + 1 + inline[0].length
        } else if (src[close + 1] === '[') {
          const refClose = brackets.get(close + 1)
          if (refClose !== undefined) {
            const name = src.slice(close + 2, refClose)
            target = lookup(refs, name || label)
            end = refClose + 1
          }
        }
        if (!target && !inline && src[close + 1] !== '[') target = lookup(refs, label)
        if (target) {
          if (image) {
            push({ kind: 'image', src: target.href, alt: plainText(parseInline(label, refs, depth + 1, true)) })
          } else {
            const children = parseInline(label, refs, depth + 1, true)
            const href = safeHref(target.href)
            if (href) push({ kind: 'link', href, ...(target.title ? { title: target.title } : {}), children })
            else children.forEach(push)
          }
          i = end
          continue
        }
      }
      text += src.slice(i, open + 1)
      i = open + 1
      continue
    }

    if (char === '*' || char === '_' || char === '~') {
      const length = runLength(src, i)
      const done = emphasis(src, i, length, refs, depth, inLink, noClose)
      if (done) {
        if (done.before) text += done.before
        push(done.node)
        i = done.end
        continue
      }
      text += src.slice(i, i + length)
      i += length
      continue
    }

    if (char === 'h' && !inLink && !/[\p{L}\p{N}]/u.test(src[i - 1] ?? ' ')) {
      const found = sticky(BARE_URL, src, i)
      if (found) {
        // Trailing punctuation ends the sentence, not the URL; so does a ) that closes nothing.
        const whole = found[0]
        const opens = whole.split('(').length
        let closes = whole.split(')').length
        let end = whole.length
        while (end > 0) {
          const last = whole[end - 1]!
          if ('.,:;!?"\'*_~'.includes(last)) end--
          else if (last === ')' && closes > opens) {
            closes--
            end--
          } else break
        }
        const url = whole.slice(0, end)
        const href = safeHref(url)
        if (href && /^https?:\/\/[^/]/.test(url)) {
          push({ kind: 'link', href, children: [{ kind: 'text', text: url }] })
          i += url.length
          continue
        }
      }
    }

    text += char
    i++
  }
  flush()
  return out
}

/** A reference by its label; labels are at most 999 characters, which also keeps hostile brackets cheap. */
function lookup(refs: Refs, label: string): { href: string, title?: string } | null {
  if (!refs.size || label.length > 999) return null
  return refs.get(normalizeLabel(label)) ?? null
}

/** The ] that closes each [, skipping escapes and code spans' insides only roughly (as GitHub does for brackets). */
function matchBrackets(src: string): Map<number, number> {
  const pairs = new Map<number, number>()
  const stack: number[] = []
  for (let i = 0; i < src.length; i++) {
    const char = src[i]
    if (char === '\\') i++
    else if (char === '[') stack.push(i)
    else if (char === ']' && stack.length) pairs.set(stack.pop()!, i)
  }
  return pairs
}

/** Emphasis, strong or strike opened by the run at `at`, or null when nothing closes it. */
function emphasis(src: string, at: number, length: number, refs: Refs, depth: number, inLink: boolean, noClose: Map<string, number>):
  { before: string, node: Inline, end: number } | null {
  const char = src[at]!
  if (char === '~') {
    if (length > 2 || !leftFlanking(src, at, length)) return null
    const close = findClose(src, at + length, char, length, true, noClose)
    if (close < 0) return null
    return { before: '', node: { kind: 'strike', children: parseInline(src.slice(at + length, close), refs, depth + 1, inLink) }, end: close + length }
  }
  if (!canOpen(src, at, length)) return null
  const inner = (from: number, to: number) => parseInline(src.slice(from, to), refs, depth + 1, inLink)
  if (length >= 3) {
    const close = findClose(src, at + length, char, 3, false, noClose)
    if (close >= 0) {
      const start = at + length
      return { before: char.repeat(length - 3), node: { kind: 'em', children: [{ kind: 'strong', children: inner(start, close) }] }, end: close + 3 }
    }
  }
  if (length >= 2) {
    const close = findClose(src, at + length, char, 2, false, noClose)
    if (close >= 0) return { before: char.repeat(length - 2), node: { kind: 'strong', children: inner(at + length, close) }, end: close + 2 }
  }
  const close = findClose(src, at + length, char, 1, false, noClose)
  if (close >= 0) return { before: char.repeat(length - 1), node: { kind: 'em', children: inner(at + length, close) }, end: close + 1 }
  return null
}

/** The first run of `char` from `from` that can close with `length` of it (exactly that length when `exact`). */
function findClose(src: string, from: number, char: string, length: number, exact: boolean, noClose: Map<string, number>): number {
  const key = `${char}${length}${exact}`
  const failedFrom = noClose.get(key)
  if (failedFrom !== undefined && from >= failedFrom) return -1
  let i = from
  while (i < src.length) {
    const at = src.indexOf(char, i)
    if (at < 0) break
    if (src[at - 1] === '\\' && at > from) {
      i = at + 1
      continue
    }
    const run = runLength(src, at)
    const fits = exact ? run === length : run >= length
    if (at > from && fits && (char === '~' ? rightFlanking(src, at, run) : canClose(src, at, run))) return at
    i = at + run
  }
  noClose.set(key, Math.min(from, failedFrom ?? from))
  return -1
}

/** The text of inline parts, without their markup (an image's alt text). */
export function plainText(nodes: Inline[]): string {
  let out = ''
  for (const node of nodes) {
    if (node.kind === 'text' || node.kind === 'code') out += node.text
    else if (node.kind === 'image') out += node.alt
    else if (node.kind === 'break') out += ' '
    else out += plainText(node.children)
  }
  return out
}
