/** The bands main.css has a colour for: named bands keep theirs. */
const PALETTE = ['kernel', 'foundation', 'content', 'publishing', 'management', 'entry', 'packages', 'api']
const NAMED = new Set([...PALETTE, 'unmapped'])

/**
 * The CSS variable holding a band's colour. A band of another project (`apps`, `libs`…) gets one of the palette's
 * colours by its name, so it keeps that colour from scan to scan; `other` and `unmapped` stay grey.
 */
export function layerVar(layer: string): string {
  if (NAMED.has(layer)) return `--layer-${layer}`
  if (layer === 'other') return '--layer-unmapped'
  let hash = 0
  for (const character of layer) hash = (hash * 31 + character.charCodeAt(0)) >>> 0
  return `--layer-${PALETTE[hash % PALETTE.length]}`
}

/** The colour of a band of the layer map, as a CSS value. */
export function layerColor(layer: string): string {
  return `var(${layerVar(layer)})`
}

let probe: CanvasRenderingContext2D | null = null

/**
 * A CSS variable's colour as `rgb()`, for the graphs: Cytoscape reads neither `var()` (Nuxt UI's tokens point at other
 * tokens) nor `oklch()` (Tailwind's greys), so the colour is resolved by the browser, painted on one pixel and read back.
 */
export function resolvedColor(name: string): string {
  const fallback = 'rgb(136, 136, 136)'
  probe ??= document.createElement('canvas').getContext('2d', { willReadFrequently: true })
  if (!probe) return fallback
  const element = document.createElement('span')
  element.style.color = `var(${name}, ${fallback})`
  document.body.appendChild(element)
  const computed = getComputedStyle(element).color
  element.remove()
  probe.clearRect(0, 0, 1, 1)
  probe.fillStyle = computed
  probe.fillRect(0, 0, 1, 1)
  const [r, g, b] = probe.getImageData(0, 0, 1, 1).data
  return `rgb(${r}, ${g}, ${b})`
}

/** A folder's own name, from a path with / or \ (a repository's, for a notification). */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path
}

/** The last two segments of a path, for graph labels. */
export function tailOf(path: string): string {
  return path.split('/').slice(-2).join('/')
}
