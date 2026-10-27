import { insets, type Box } from './canvas.ts'

/*
 * Pictures of the canvas and the layer map as SVG, for Export. The whole map, not the part in view: every box of the
 * canvas's tree as a rectangle in the canvas's colours, with its name where the canvas would write it at this size,
 * and every band of the layer map with its parts as tiles. Plain strings, so the window rasterises them for a PNG.
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

export interface CanvasSvgOptions {
  /** The picture's width in pixels; its height follows the world's shape. */
  width: number
  colors: SvgColors
  fonts?: SvgFonts
}

/**
 * The canvas's tree (laid out by `layout`) as SVG, in world units with the root's rectangle as the viewBox. A box's
 * fill is the canvas's: parts and folders a light wash of their band, files the raised background, functions their band
 * deeper by nesting. A name is written where the canvas writes it at this scale: the box at least 36 pixels wide and the
 * space above its children (a function without children: its top) at least 8 pixels tall.
 */
export function canvasSvg(root: Box, options: CanvasSvgOptions): string {
  const { colors } = options
  const fonts = options.fonts ?? FONTS
  const k = options.width / (root.w || 1)
  const height = Math.round(root.h * k)
  const out: string[] = []
  const walk = (item: Box) => {
    const layer = colors.layer(item.layer)
    let fill = layer
    let opacity = 1
    if (item.kind === 'part') opacity = 0.1
    else if (item.kind === 'folder') opacity = 0.06
    else if (item.kind === 'file') fill = colors.elevated
    else opacity = 0.26 + Math.min(0.24, (item.depth % 4) * 0.06)
    const stroke = item.kind === 'part' ? ` stroke="${escapeXml(layer)}" stroke-opacity="0.6"` : ` stroke="${escapeXml(colors.border)}"`
    out.push(`<rect x="${num(item.x)}" y="${num(item.y)}" width="${num(item.w)}" height="${num(item.h)}" fill="${escapeXml(fill)}"${opacity < 1 ? ` fill-opacity="${num(opacity)}"` : ''}${stroke}/>`)

    const { top, side } = insets(item)
    const w = item.w * k
    const header = item.children.length ? top * k : Math.min(item.h * k, 22)
    if (w > 36 && header >= 8) {
      const size = Math.max(9, Math.min(item.kind === 'part' ? 15 : 13, header * 0.72))
      const pad = Math.max(3, side * k)
      const mono = item.kind === 'function'
      const label = fitted(item.label, w - pad * 2, size, mono ? 0.6 : 0.55)
      if (label) {
        const kind = mono ? 'm f' : item.kind === 'part' ? 's b' : 's'
        const color = item.kind === 'folder' ? colors.muted : colors.text
        out.push(`<text class="${kind}" x="${num(item.x + pad / k)}" y="${num(item.y + (Math.min(header, 22) / 2 + 1) / k)}" font-size="${num(size / k)}" fill="${escapeXml(color)}">${escapeXml(label)}</text>`)
      }
    }
    item.children.forEach(walk)
  }
  root.children.forEach(walk)
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.round(options.width)}" height="${height}" viewBox="${num(root.x)} ${num(root.y)} ${num(root.w)} ${num(root.h)}">`,
    style(fonts),
    `<rect x="${num(root.x)}" y="${num(root.y)}" width="${num(root.w)}" height="${num(root.h)}" fill="${escapeXml(colors.background)}"/>`,
    `<g stroke-width="${num(1 / k)}">`,
    ...out,
    '</g>',
    '</svg>'
  ].join('\n')
}

/** A band of the layer map: its name and size, and its parts, with their sizes as already-translated texts. */
export interface SvgBand {
  layer: string
  label: string
  size: string
  tiles: { name: string, size: string }[]
}

export interface LayersSvgOptions {
  /** The picture's width in pixels; its height follows the bands. */
  width?: number
  colors: SvgColors
  fonts?: SvgFonts
}

const PAD = 16
const LABEL = 180
const GAP = 6
const TILE = { min: 120, max: 320, h: 48 }

/**
 * The layer map as SVG, as the layers page shows it: a band per layer, top to bottom, with its name and size on the
 * left behind a stripe of its colour, and its parts as tiles in rows beside it, each with a bar of the band's colour on
 * top, its name and its size. Tiles are as wide as their names (within bounds), and each row is stretched to fill.
 */
export function layersSvg(bands: SvgBand[], options: LayersSvgOptions): string {
  const { colors } = options
  const fonts = options.fonts ?? FONTS
  const width = Math.round(options.width ?? 1200)
  const room = width - PAD * 2 - LABEL - GAP * 2
  const out: string[] = []
  let y = PAD
  for (const band of bands) {
    const color = escapeXml(colors.layer(band.layer))
    // The tiles in rows, each row as many as fit.
    const rows: { name: string, size: string, w: number }[][] = [[]]
    let used = 0
    for (const tile of band.tiles) {
      const w = Math.min(TILE.max, Math.max(TILE.min, Math.max(tile.name.length * 7.2, tile.size.length * 6) + 20), room)
      const row = rows.at(-1)!
      if (row.length && used + GAP + w > room) {
        rows.push([])
        used = 0
      }
      used += (rows.at(-1)!.length ? GAP : 0) + w
      rows.at(-1)!.push({ ...tile, w })
    }
    const height = Math.max(TILE.h, rows.length * (TILE.h + GAP) - GAP)
    out.push(
      `<rect x="${PAD}" y="${y}" width="${LABEL}" height="${height}" rx="4" fill="${escapeXml(colors.elevated)}"/>`,
      `<rect x="${PAD}" y="${y}" width="4" height="${height}" fill="${color}"/>`,
      `<text class="s b" x="${PAD + 12}" y="${y + height / 2 - 8}" font-size="12" letter-spacing="0.06em" fill="${color}">${escapeXml(fitted(band.label.toUpperCase(), LABEL - 18, 12, 0.7))}</text>`,
      `<text class="s" x="${PAD + 12}" y="${y + height / 2 + 9}" font-size="11" fill="${escapeXml(colors.muted)}">${escapeXml(fitted(band.size, LABEL - 18, 11, 0.55))}</text>`
    )
    rows.forEach((row, at) => {
      const natural = row.reduce((sum, tile) => sum + tile.w, 0) + GAP * (row.length - 1)
      const stretch = row.length ? (room - natural) / row.reduce((sum, tile) => sum + tile.w, 0) : 0
      let x = PAD + LABEL + GAP * 2
      const top = y + at * (TILE.h + GAP)
      for (const tile of row) {
        const w = tile.w * (1 + stretch)
        out.push(
          `<rect x="${num(x)}" y="${top}" width="${num(w)}" height="${TILE.h}" rx="4" fill="${escapeXml(colors.background)}" stroke="${escapeXml(colors.border)}"/>`,
          `<rect x="${num(x)}" y="${top}" width="${num(w)}" height="3" fill="${color}"/>`,
          `<text class="m f" x="${num(x + 8)}" y="${top + 18}" font-size="12" fill="${escapeXml(colors.text)}">${escapeXml(fitted(tile.name, w - 16, 12, 0.6))}</text>`,
          `<text class="s" x="${num(x + 8)}" y="${top + 35}" font-size="11" fill="${escapeXml(colors.muted)}">${escapeXml(fitted(tile.size, w - 16, 11, 0.55))}</text>`
        )
        x += w + GAP
      }
    })
    y += height + GAP * 2
  }
  const total = Math.max(PAD * 2, y - GAP * 2 + PAD)
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${total}" viewBox="0 0 ${width} ${total}">`,
    style(fonts),
    `<rect width="${width}" height="${total}" fill="${escapeXml(colors.background)}"/>`,
    ...out,
    '</svg>'
  ].join('\n')
}
