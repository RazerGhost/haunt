import { contrast, hexOf, rampFrom, rampName } from './ramp.ts'
import { presets, STOCK, type ColorAlias, type ShadeStop, type ThemeDoc, type VariantGroup } from './themes.ts'

/*
 * A profile's look, as the window applies it: its theme (shared/themes.ts) with the profile's own choices on top, each
 * empty choice the theme's. Out come Nuxt UI's colours for light and for dark (the window swaps them as it turns), one
 * stylesheet (custom colour ramps, the semantic `--ui-*` tokens per mode, the corners, the fonts), the root font size,
 * the icon set and the components' app config (default variants, sizes and colours, and the theme's own overrides).
 * Modelled on Nuxt UI's theme studio (its docs' theme engine), for the parts a theme may set.
 */

/** A profile's look as Rust keeps it (settings.rs `Appearance`); every field empty is the theme's. A colour is a
 * Tailwind name or a hex (`#2b7fff`), whose shades shared/ramp.ts makes. */
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
  /** A theme file's colours for the window's own parts (theme_file.rs), each a hex, over the shades. */
  roles?: Partial<Record<Role, string>>
  /** A theme file's background opacity, 0 to 1, for the compositor's blur behind a transparent window (Linux). */
  opacity?: number | null
}

export type Role = 'background' | 'raised' | 'text' | 'muted' | 'border' | 'primary' | 'onPrimary'

/** Nuxt UI's tokens each role sets: the role's colour, or a mix of it with another role's (`[colour, share, other]`). */
const ROLE_TOKENS: Record<Role, [string, Role | [Role, number, Role]][]> = {
  background: [['--ui-bg', 'background']],
  raised: [['--ui-bg-elevated', 'raised'], ['--ui-bg-muted', ['raised', 50, 'background']], ['--ui-bg-accented', ['raised', 85, 'text']]],
  text: [['--ui-text', 'text'], ['--ui-text-highlighted', 'text'], ['--ui-bg-inverted', 'text']],
  muted: [['--ui-text-muted', 'muted'], ['--ui-text-toned', ['muted', 50, 'text']], ['--ui-text-dimmed', ['muted', 70, 'background']]],
  border: [['--ui-border', 'border'], ['--ui-border-muted', 'border'], ['--ui-border-accented', ['border', 80, 'text']]],
  primary: [['--ui-primary', 'primary']],
  onPrimary: [['--ui-text-inverted', 'onPrimary']]
}

/** The tokens a theme file's roles set; a mix needs both of its roles. */
function roleTokens(roles: Partial<Record<Role, string>>): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [role, tokens] of Object.entries(ROLE_TOKENS) as [Role, (typeof ROLE_TOKENS)[Role]][]) {
    if (!roles[role]) continue
    for (const [token, from] of tokens) {
      if (typeof from === 'string') out[token] = roles[from]!
      else if (roles[from[2]]) out[token] = `color-mix(in oklab, ${roles[from[0]]} ${from[1]}%, ${roles[from[2]]})`
    }
  }
  return out
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

/** `accent` is the desktop's (desktop.rs), for a main colour of "system"; without one that is the theme's. */
export function applyLook(look: Look, accent?: string | null): Applied {
  const doc = themeOf(look.theme).doc
  const system = (value: string | undefined) => (value === 'system' ? accent ?? '' : value)
  look = { ...look, primary: system(look.primary), darkPrimary: system(look.darkPrimary) }
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
    // A hex is named by its shades (below).
    for (const alias of Object.keys(out) as ColorAlias[]) {
      const hex = hexOf(out[alias])
      if (hex) out[alias] = rampName(hex)
    }
    return out
  }
  const colors = { light: roles(false), dark: roles(true) }

  const css: string[] = []
  // Custom ramps, as the `--color-*` Nuxt UI's colours point at.
  const ramps: Record<string, string> = {}
  for (const [name, palette] of Object.entries(doc.palettes ?? {})) {
    for (const [step, value] of Object.entries(palette.shades)) ramps[`--color-${name}-${step}`] = value
  }
  // And one for each hex the look names.
  for (const name of new Set([...Object.values(colors.light), ...Object.values(colors.dark)])) {
    if (!name.startsWith('hex-')) continue
    for (const [step, value] of Object.entries(rampFrom(`#${name.slice(4)}`))) ramps[`--color-${name}-${step}`] = value
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
  // A main colour given as a hex is the main colour itself, not the shade beside it, where it reads on the page (3:1).
  for (const [mode, chosen, page] of [['light', look.primary, '#ffffff'], ['dark', look.darkPrimary, '#171717']] as const) {
    const hex = hexOf(chosen ?? '')
    if (hex && contrast(hex, page) >= 3) tokens[mode]['--ui-primary'] = hex
  }
  // A theme file's roles, the same in both modes (a tool writes one at a time), then its opacity.
  Object.assign(tokens.light, roleTokens(look.roles ?? {}))
  Object.assign(tokens.dark, roleTokens(look.roles ?? {}))
  if (look.opacity != null && look.opacity < 1) {
    const share = `${Math.round(look.opacity * 100)}%`
    tokens.light['--ui-bg'] = `color-mix(in srgb, ${tokens.light['--ui-bg'] ?? '#fff'} ${share}, transparent)`
    tokens.dark['--ui-bg'] = `color-mix(in srgb, ${tokens.dark['--ui-bg'] ?? 'var(--ui-color-neutral-900)'} ${share}, transparent)`
  }
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
    colors,
    plain,
    css: css.filter(Boolean).join('\n'),
    fontSize: Number(look.fontSize) || doc.fontSize || STOCK.fontSize,
    icons: look.icons || doc.icons || STOCK.icons,
    fonts: [sans, doc.font?.mono, doc.font?.serif].filter((name): name is string => !!name && name !== 'system'),
    components
  }
}
