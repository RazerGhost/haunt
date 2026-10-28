/*
 * Shades from one colour: a hex (a desktop's accent, a wallpaper tool's colour, one picked in Appearance) made into the
 * eleven steps a Tailwind palette has, 50 to 950, so Nuxt UI can use it as it uses `blue` or `slate`. Worked out in
 * OKLCH: each step's lightness and colourfulness are spaced the way Tailwind's own palettes are (their average over
 * the hues), the hue stays the colour's own, and the colour itself is kept exactly as the step nearest its lightness.
 * The steps beside it move with it, less the further they are, so a light or dark colour does not leave a gap.
 */

export const STEPS = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const
export type Step = (typeof STEPS)[number]

/** Tailwind's palettes on average: each step's lightness, and its colourfulness against the 500's. */
const LIGHTNESS = [0.971, 0.934, 0.884, 0.809, 0.706, 0.63, 0.562, 0.497, 0.434, 0.388, 0.27]
const CHROMA = [0.06, 0.14, 0.27, 0.48, 0.79, 1, 1.08, 1.02, 0.84, 0.64, 0.41]

/** A colour as `#rrggbb` (or `#rgb`), lower case; null when it is not one. */
export function hexOf(value: string): string | null {
  const text = value.trim().toLowerCase()
  if (/^#[0-9a-f]{6}$/.test(text)) return text
  if (/^#[0-9a-f]{3}$/.test(text)) return `#${[...text.slice(1)].map(digit => digit + digit).join('')}`
  return null
}

const toLinear = (channel: number) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4)
const fromLinear = (channel: number) => (channel <= 0.0031308 ? 12.92 * channel : 1.055 * channel ** (1 / 2.4) - 0.055)

/** Red, green and blue from 0 to 1. */
export function rgbOf(hex: string): [number, number, number] {
  const value = Number.parseInt(hex.slice(1), 16)
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255].map(channel => channel / 255) as [number, number, number]
}

export function hexFrom([red, green, blue]: [number, number, number]): string {
  return `#${[red, green, blue].map(channel => Math.round(Math.min(1, Math.max(0, channel)) * 255).toString(16).padStart(2, '0')).join('')}`
}

/** OKLCH (lightness 0 to 1, chroma, hue in degrees) of an sRGB colour. */
export function oklchOf(rgb: [number, number, number]): [number, number, number] {
  const [r, g, b] = rgb.map(toLinear) as [number, number, number]
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
  const lightness = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
  const hue = (Math.atan2(bb, a) * 180) / Math.PI
  return [lightness, Math.hypot(a, bb), hue < 0 ? hue + 360 : hue]
}

/** The sRGB colour of an OKLCH one; channels may fall outside 0 to 1 when it is out of sRGB's range. */
function rgbFromOklch([lightness, chroma, hue]: [number, number, number]): [number, number, number] {
  const a = chroma * Math.cos((hue * Math.PI) / 180)
  const b = chroma * Math.sin((hue * Math.PI) / 180)
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s
  ].map(fromLinear) as [number, number, number]
}

const inside = (rgb: number[]) => rgb.every(channel => channel >= -0.0005 && channel <= 1.0005)

/** The colour at this lightness and hue, as colourful as asked or as sRGB allows. */
function fitted(lightness: number, chroma: number, hue: number): string {
  if (inside(rgbFromOklch([lightness, chroma, hue]))) return hexFrom(rgbFromOklch([lightness, chroma, hue]))
  let low = 0
  let high = chroma
  for (let round = 0; round < 24; round++) {
    const middle = (low + high) / 2
    if (inside(rgbFromOklch([lightness, middle, hue]))) low = middle
    else high = middle
  }
  return hexFrom(rgbFromOklch([lightness, low, hue]))
}

/** The eleven shades of a colour, `#rrggbb` each; the colour itself is one of them. */
export function rampFrom(hex: string): Record<Step, string> {
  const [lightness, chroma, hue] = oklchOf(rgbOf(hex))
  let anchor = 0
  for (let index = 1; index < STEPS.length; index++) {
    if (Math.abs(LIGHTNESS[index]! - lightness) < Math.abs(LIGHTNESS[anchor]! - lightness)) anchor = index
  }
  const shift = lightness - LIGHTNESS[anchor]!
  const out = {} as Record<Step, string>
  STEPS.forEach((step, index) => {
    if (index === anchor) {
      out[step] = hex
      return
    }
    // The steps beside the colour move with it, the ends not at all.
    const reach = index < anchor ? anchor : STEPS.length - 1 - anchor
    const weight = 1 - Math.abs(index - anchor) / reach
    out[step] = fitted(LIGHTNESS[index]! + shift * weight, (chroma * CHROMA[index]!) / CHROMA[anchor]!, hue)
  })
  return out
}

/** The name Nuxt UI knows a hex's shades by (`--color-<name>-500`): `hex-2b7fff`. */
export const rampName = (hex: string) => `hex-${hex.slice(1)}`

/** WCAG's contrast between two colours, 1 to 21. */
export function contrast(first: string, second: string): number {
  const luminance = (hex: string) => {
    const [r, g, b] = rgbOf(hex).map(toLinear) as [number, number, number]
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
  }
  const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a) as [number, number]
  return (lighter + 0.05) / (darker + 0.05)
}
