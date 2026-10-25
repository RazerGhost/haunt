import { presets, STOCK, type ColorAlias, type ShadeStop, type ThemeDoc, type VariantGroup } from './themes.ts'

/*
 * A profile's look, as the window applies it: its theme (shared/themes.ts) with the profile's own choices on top, each
 * empty choice the theme's. Out come Nuxt UI's colours for light and for dark (the window swaps them as it turns), one
 * stylesheet (custom colour ramps, the semantic `--ui-*` tokens per mode, the corners, the fonts), the root font size,
 * the icon set and the components' app config (default variants, sizes and colours, and the theme's own overrides).
 * Modelled on Nuxt UI's theme studio (its docs' theme engine), for the parts a theme may set.
 */

/** A profile's look as Rust keeps it (settings.rs `Appearance`); every field empty is the theme's. */
export interface Look {
  mode?: string
  theme?: string
  primary?: string
  neutral?: string
  darkPrimary?: string
  darkNeutral?: string
  secondary?: string
  success?: string
  info?: string
  warning?: string
  error?: string
  radius?: string
  font?: string
  icons?: string
  fontSize?: string
  buttons?: string
  inputs?: string
  panels?: string
  size?: string
}

export interface Applied {
  colors: { light: Record<ColorAlias, string>, dark: Record<ColorAlias, string> }
  /** The main colour is plain black (light) or white (dark). */
  plain: { light: boolean, dark: boolean }
  css: string
  fontSize: number
  icons: string
  /** The font families the look names (not the system's): used when installed, or loaded from Google Fonts. */
  fonts: string[]
  /** `app.config ui.<component>` for every component the look changes. */
  components: Record<string, Record<string, unknown>>
}

const SEMANTIC = ['secondary', 'success', 'info', 'warning', 'error'] as const
const SYSTEM_SANS = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif'
const FIELDS = ['input', 'select', 'textarea', 'selectMenu', 'inputMenu', 'inputNumber', 'inputTags', 'inputDate', 'inputTime', 'pinInput']
/** Which components take which default variant (Nuxt UI's studio tables). */
const VARIANT_SUPPORT: Record<string, string[]> = {
  button: ['solid', 'outline', 'soft', 'subtle', 'ghost', 'link'],
  badge: ['solid', 'outline', 'soft', 'subtle'],
  alert: ['solid', 'outline', 'soft', 'subtle'],
  card: ['solid', 'outline', 'soft', 'subtle'],
  empty: ['solid', 'outline', 'soft', 'subtle'],
  ...Object.fromEntries(FIELDS.map(component => [component, ['outline', 'soft', 'subtle', 'ghost', 'none']]))
}
const VARIANT_GROUPS: Record<VariantGroup, string[]> = { buttons: ['button', 'badge'], panels: ['card', 'alert', 'empty'], inputs: FIELDS }
const SIZE_SUPPORT = [
  'button', 'badge', ...FIELDS, 'inputRating', 'tabs', 'checkbox', 'checkboxGroup', 'radioGroup', 'switch', 'slider', 'stepper',
  'calendar', 'colorPicker', 'fileUpload', 'formField', 'fieldGroup', 'dropdownMenu', 'contextMenu', 'commandPalette', 'listbox'
]
const COLOR_SUPPORT = ['button', 'badge', ...FIELDS]

export function themeOf(id: string | undefined) {
  return presets.find(preset => preset.id === (id || 'haunt')) ?? presets[0]!
}

/** A token's ramp: `--ui-primary` (and the other colour tokens) its own colour, everything else the neutral greys. */
function rampOf(token: string): string {
  const name = token.replace(/^--ui-/, '')
  return ['primary', ...SEMANTIC].includes(name) ? name : 'neutral'
}
function shade(ramp: string, stop: ShadeStop): string {
  return stop === 'white' || stop === 'black' ? stop : `var(--ui-color-${ramp}-${stop})`
}
const block = (selector: string, rules: Record<string, string | number>) =>
  Object.keys(rules).length ? `${selector} { ${Object.entries(rules).map(([name, value]) => `${name}: ${value};`).join(' ')} }` : ''
const family = (name: string, fallback: string) => (name === 'system' ? SYSTEM_SANS : `"${name}", ${fallback}`)

function merge(base: Record<string, unknown>, extra: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = { ...base }
  for (const [key, value] of Object.entries(extra)) {
    const known = out[key]
    if (Array.isArray(known) && Array.isArray(value)) out[key] = [...known, ...value]
    else if (known && value && typeof known === 'object' && typeof value === 'object') out[key] = merge(known as Record<string, unknown>, value as Record<string, unknown>)
    else out[key] = value
  }
  return out
}

/** The default variants, sizes and colours, per component, from the theme's and the profile's choices. */
function defaultsOf(doc: ThemeDoc, look: Look): Record<string, Record<string, unknown>> {
  const chosen = doc.style?.defaults ?? {}
  const variants: Partial<Record<VariantGroup, string>> = { ...chosen.variants }
  for (const group of ['buttons', 'inputs', 'panels'] as const) if (look[group]) variants[group] = look[group]
  const size = look.size || chosen.size
  const out: Record<string, Record<string, unknown>> = {}
  const set = (component: string, key: string, value: string) => {
    out[component] = merge(out[component] ?? {}, { defaultVariants: { [key]: value } })
  }
  for (const [group, components] of Object.entries(VARIANT_GROUPS) as [VariantGroup, string[]][]) {
    const variant = variants[group] ?? chosen.variant
    for (const component of components) if (variant && VARIANT_SUPPORT[component]?.includes(variant)) set(component, 'variant', variant)
    const color = chosen.colors?.[group]
    for (const component of components) if (color && COLOR_SUPPORT.includes(component)) set(component, 'color', color)
  }
  if (size) for (const component of SIZE_SUPPORT) set(component, 'size', size)
  return out
}

export function applyLook(look: Look): Applied {
  const doc = themeOf(look.theme).doc
  const plain = {
    light: look.primary ? look.primary === 'neutral' : !!doc.blackAsPrimary,
    dark: look.darkPrimary ? look.darkPrimary === 'neutral' : !!doc.blackAsPrimary
  }
  const roles = (dark: boolean): Record<ColorAlias, string> => {
    const out = { ...STOCK.colors, ...doc.colors }
    for (const alias of SEMANTIC) if (look[alias]) out[alias] = look[alias]!
    const primary = dark ? look.darkPrimary : look.primary
    const neutral = dark ? look.darkNeutral : look.neutral
    if (primary) out.primary = primary
    if (neutral) out.neutral = neutral
    // A plain main colour still needs a ramp for its tints: the greys'.
    if (dark ? plain.dark : plain.light) out.primary = 'neutral'
    return out
  }

  const css: string[] = []
  // Custom ramps, as the `--color-*` Nuxt UI's colours point at.
  const ramps: Record<string, string> = {}
  for (const [name, palette] of Object.entries(doc.palettes ?? {})) {
    for (const [step, value] of Object.entries(palette.shades)) ramps[`--color-${name}-${step}`] = value
  }
  css.push(block(':root', ramps))
  // The semantic tokens per mode: the theme's shades, then its raw values, then a plain main colour. Higher than Nuxt
  // UI's own `:root` and `.dark` rules, so they win wherever the stylesheets land.
  const tokens = { light: {} as Record<string, string>, dark: {} as Record<string, string> }
  for (const [token, stops] of Object.entries(doc.style?.tokenShades ?? {})) {
    if (stops.light !== undefined) tokens.light[token] = shade(rampOf(token), stops.light)
    if (stops.dark !== undefined) tokens.dark[token] = shade(rampOf(token), stops.dark)
  }
  Object.assign(tokens.light, doc.tokens?.light)
  Object.assign(tokens.dark, doc.tokens?.dark)
  if (plain.light) tokens.light['--ui-primary'] = 'black'
  if (plain.dark) tokens.dark['--ui-primary'] = 'white'
  css.push(block('html:root:not(.dark)', tokens.light), block('html:root.dark', tokens.dark))

  // Corners and fonts.
  const radius = look.radius ? Number(look.radius) : doc.radius ?? STOCK.radius
  const sans = look.font || doc.font?.sans || STOCK.font
  const root: Record<string, string | number> = { '--ui-radius': `${radius}rem`, '--font-sans': family(sans, SYSTEM_SANS) }
  if (doc.font?.mono) root['--font-mono'] = family(doc.font.mono, 'ui-monospace, monospace')
  if (doc.font?.serif) root['--font-serif'] = family(doc.font.serif, 'ui-serif, Georgia, serif')
  for (const [step, weight] of Object.entries(doc.font?.weights ?? {})) root[`--font-weight-${step}`] = weight
  css.push(block('html:root', root))
  const body: Record<string, string | number> = {}
  if (doc.font?.lineHeight) body['line-height'] = doc.font.lineHeight
  if (doc.font?.letterSpacing) body['letter-spacing'] = `${doc.font.letterSpacing}em`
  if (doc.font?.weights?.normal) body['font-weight'] = doc.font.weights.normal
  css.push(block('html:root body', body))
  if (doc.font?.serif) css.push('h1, h2, h3 { font-family: var(--font-serif); }')

  let components = defaultsOf(doc, look)
  for (const [name, fragment] of Object.entries(doc.components ?? {})) components = { ...components, [name]: merge(components[name] ?? {}, fragment) }

  return {
    colors: { light: roles(false), dark: roles(true) },
    plain,
    css: css.filter(Boolean).join('\n'),
    fontSize: Number(look.fontSize) || doc.fontSize || STOCK.fontSize,
    icons: look.icons || doc.icons || STOCK.icons,
    fonts: [sans, doc.font?.mono, doc.font?.serif].filter((name): name is string => !!name && name !== 'system'),
    components
  }
}
