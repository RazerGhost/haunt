/*
 * A picture of the code graph as SVG, for Export. The whole graph, not the part in view: every card with its rows, and
 * every line. A plain string, so the window rasterises it for a PNG.
 * Colours come resolved from the theme (light or dark is the window's choice) and texts already translated.
 */

/** The theme's colours, as `rgb(…)` or any SVG colour; `layer` gives a band's. */
export interface SvgColors {
  background: string
  elevated: string
  border: string
  text: string
  muted: string
  layer: (layer: string) => string
}

export interface SvgFonts {
  sans: string
  mono: string
}

const FONTS: SvgFonts = { sans: 'system-ui, sans-serif', mono: 'ui-monospace, monospace' }

/** Text for XML: in an element or an attribute. */
export function escapeXml(text: string): string {
  return text.replace(/[&<>"']/g, character => `&${({ '&': 'amp', '<': 'lt', '>': 'gt', '"': 'quot', '\'': 'apos' } as Record<string, string>)[character]};`)
}

/** A number short: two decimals at most. */
const num = (value: number) => String(Math.round(value * 100) / 100)

/** `text` cut with … to about `max` wide at `size`; empty when not even two letters fit. SVG has no measuring, so a
 * letter is taken as `per` of the font size wide. */
function fitted(text: string, max: number, size: number, per: number): string {
  const room = Math.floor(max / (size * per))
  if (room >= text.length) return text
  return room >= 3 ? `${text.slice(0, room - 1)}…` : ''
}

function style(fonts: SvgFonts): string {
  return `<style>text{dominant-baseline:middle}.s{font-family:${escapeXml(fonts.sans)}}.m{font-family:${escapeXml(fonts.mono)}}.b{font-weight:600}.f{font-weight:500}</style>`
}

/** A card of the code graph as drawn (CodeGraph.vue), its texts already translated. */
export interface SvgGraphCard {
  x: number
  y: number
  w: number
  h: number
  title: string
  /** After the title, muted: the file's folder, or how many rows a shut card holds. */
  folder: string
  /** The band its dot is coloured by; none without a dot. */
  layer: string | null
  dashed: boolean
  /** Where the first row starts below the card's top, and how tall each row is. */
  rowsTop: number
  row: number
  rows: { label: string, letter: string, folder: string, depth: number, context: boolean, notes: string[] }[]
  /** The line under the rows ("3 more · Show"), if any. */
  footer: string
}

/** A line of the code graph: one cubic curve (from, two control points, to), in its colour. */
export interface SvgGraphLine {
  curve: [number, number, number, number, number, number, number, number]
  color: string
  width: number
  dashed: boolean
  opacity: number
}

export interface GraphSvgOptions {
  colors: SvgColors
  fonts?: SvgFonts
  /** At most this wide in pixels: a bigger graph is drawn smaller. */
  maxWidth?: number
}

const ARROW = 7
const MARGIN = 24

/**
 * The code graph as SVG, as the Graph page draws it: the lines under the cards, each with its arrowhead, and each card
 * with its header (a dot of its band, its name, its folder) and its rows (kind letter, name, notes on the right).
 */
export function graphSvg(cards: SvgGraphCard[], lines: SvgGraphLine[], options: GraphSvgOptions): string {
  const { colors } = options
  const fonts = options.fonts ?? FONTS
  const left = Math.min(...cards.map(card => card.x)) - MARGIN
  const top = Math.min(...cards.map(card => card.y)) - MARGIN
  const w = Math.max(...cards.map(card => card.x + card.w)) + MARGIN - left
  const h = Math.max(...cards.map(card => card.y + card.h)) + MARGIN - top
  const k = Math.min(1, (options.maxWidth ?? Infinity) / w)
  const out: string[] = []
  for (const line of lines) {
    const [x0, y0, ax, ay, bx, by, x1, y1] = line.curve
    const color = escapeXml(line.color)
    const angle = Math.atan2(y1 - by, x1 - bx)
    const cos = Math.cos(angle)
    const sin = Math.sin(angle)
    const tip = [x1 - ARROW * cos + ARROW * 0.45 * sin, y1 - ARROW * sin - ARROW * 0.45 * cos, x1 - ARROW * cos - ARROW * 0.45 * sin, y1 - ARROW * sin + ARROW * 0.45 * cos].map(num)
    out.push(
      `<g opacity="${num(line.opacity)}"><path d="M${num(x0)},${num(y0)} C${num(ax)},${num(ay)} ${num(bx)},${num(by)} ${num(x1)},${num(y1)}" fill="none" stroke="${color}" stroke-width="${num(line.width)}"${line.dashed ? ' stroke-dasharray="5 4"' : ''}/>`,
      `<path d="M${num(x1)},${num(y1)} L${tip[0]},${tip[1]} L${tip[2]},${tip[3]} z" fill="${color}"/></g>`
    )
  }
  for (const card of cards) {
    out.push(`<rect x="${num(card.x)}" y="${num(card.y)}" width="${num(card.w)}" height="${num(card.h)}" rx="8" fill="${escapeXml(colors.elevated)}" stroke="${escapeXml(colors.border)}"${card.dashed ? ' stroke-dasharray="4 3"' : ''}/>`)
    const middle = card.y + Math.min(card.rowsTop || card.h, 36) / 2
    let x = card.x + 10
    if (card.layer !== null) {
      out.push(`<circle cx="${num(x + 3)}" cy="${num(middle)}" r="4" fill="${escapeXml(colors.layer(card.layer))}"/>`)
      x += 12
    }
    if (card.title) {
      const title = fitted(card.title, card.x + card.w - 10 - x, 12, 0.6)
      out.push(`<text class="m b" x="${num(x)}" y="${num(middle)}" font-size="12" fill="${escapeXml(colors.text)}">${escapeXml(title)}</text>`)
      const after = x + title.length * 12 * 0.6 + 6
      const folder = fitted(card.folder, card.x + card.w - 10 - after, 10, 0.6)
      if (folder) out.push(`<text class="m" x="${num(after)}" y="${num(middle)}" font-size="10" fill="${escapeXml(colors.muted)}">${escapeXml(folder)}</text>`)
    }
    if (card.rows.length && card.rowsTop) out.push(`<line x1="${num(card.x)}" x2="${num(card.x + card.w)}" y1="${num(card.y + card.rowsTop)}" y2="${num(card.y + card.rowsTop)}" stroke="${escapeXml(colors.border)}"/>`)
    card.rows.forEach((row, at) => {
      const y = card.y + card.rowsTop + at * card.row + card.row / 2
      let rx = card.x + 8 + row.depth * 14
      if (row.letter) {
        out.push(`<text class="m b" x="${num(rx)}" y="${num(y)}" font-size="10" fill="${escapeXml(colors.muted)}">${escapeXml(row.letter)}</text>`)
        rx += 14
      }
      const notes = row.notes.join(' · ')
      const room = card.x + card.w - 8 - rx - (notes ? notes.length * 10 * 0.55 + 8 : 0)
      const label = fitted(row.label, room, 12, 0.6)
      out.push(`<text class="m" x="${num(rx)}" y="${num(y)}" font-size="12" fill="${escapeXml(row.context ? colors.muted : colors.text)}">${escapeXml(label)}</text>`)
      const folder = row.folder ? fitted(row.folder, room - label.length * 12 * 0.6 - 6, 10, 0.6) : ''
      if (folder) out.push(`<text class="m" x="${num(rx + label.length * 12 * 0.6 + 6)}" y="${num(y)}" font-size="10" fill="${escapeXml(colors.muted)}">${escapeXml(folder)}</text>`)
      if (notes) out.push(`<text class="s" x="${num(card.x + card.w - 8)}" y="${num(y)}" font-size="10" text-anchor="end" fill="${escapeXml(colors.muted)}">${escapeXml(notes)}</text>`)
    })
    if (card.footer) out.push(`<text class="s" x="${num(card.x + 8)}" y="${num(card.y + card.h - 13)}" font-size="11" fill="${escapeXml(colors.muted)}">${escapeXml(fitted(card.footer, card.w - 16, 11, 0.55))}</text>`)
  }
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.round(w * k)}" height="${Math.round(h * k)}" viewBox="${num(left)} ${num(top)} ${num(w)} ${num(h)}">`,
    style(fonts),
    `<rect x="${num(left)}" y="${num(top)}" width="${num(w)}" height="${num(h)}" fill="${escapeXml(colors.background)}"/>`,
    ...out,
    '</svg>'
  ].join('\n')
}
