/*
 * The themes a profile can pick in Settings, Appearance: Haunt's own look, then the presets of Nuxt UI's theme studio,
 * copied from its documentation's source (nuxt/ui, docs/app/utils/theme/engine/presets.ts at 5fd94e1, MIT licence,
 * Copyright (c) 2023 NuxtLabs). A theme is a sparse document: what it leaves out is Nuxt UI's stock value
 * (`STOCK`). shared/theme.ts turns a theme and a profile's own choices into CSS and app config.
 */

export type Shade = 50 | 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900 | 950
export type ShadeStop = 'white' | Shade | 'black'
export type ColorAlias = 'primary' | 'secondary' | 'success' | 'info' | 'warning' | 'error' | 'neutral'
export type VariantGroup = 'buttons' | 'panels' | 'inputs'

export interface ThemeStyle {
  defaults?: { variant?: string, size?: string, variants?: Partial<Record<VariantGroup, string>>, colors?: Partial<Record<VariantGroup, string>> }
  /** A semantic token's shade per mode, on the ramp its name points at (`--ui-primary`: primary; the rest: neutral). */
  tokenShades?: Record<string, { light?: ShadeStop, dark?: ShadeStop }>
}

export interface ThemeDoc {
  version: 1
  /** Custom colour ramps, as `--color-{name}-{shade}`. */
  palettes?: Record<string, { shades: Partial<Record<Shade, string>> }>
  colors?: Partial<Record<ColorAlias, string>>
  /** The main colour is black in light mode and white in dark. */
  blackAsPrimary?: boolean
  /** Semantic `--ui-*` tokens per mode, as raw values. */
  tokens?: { light?: Record<string, string>, dark?: Record<string, string> }
  radius?: number
  /** The root font size in px. */
  fontSize?: number
  /** `sans: 'system'` is the system's own fonts. */
  font?: { sans?: string, serif?: string, mono?: string, weights?: { normal?: number, medium?: number, semibold?: number, bold?: number }, letterSpacing?: number, lineHeight?: number }
  icons?: string
  style?: ThemeStyle
  /** Per-component overrides, merged into `app.config ui.<component>`. */
  components?: Record<string, Record<string, unknown>>
}

export interface ThemePreset {
  id: string
  name: string
  description: string
  doc: ThemeDoc
}

/** What a theme leaves out: Nuxt UI's own values. */
export const STOCK = {
  colors: { primary: 'green', secondary: 'blue', success: 'green', info: 'blue', warning: 'yellow', error: 'red', neutral: 'slate' } as Record<ColorAlias, string>,
  radius: 0.25,
  fontSize: 16,
  font: 'Public Sans',
  icons: 'lucide'
}

/** The choices beside the themes, as in Nuxt UI's theme studio. */
export const NEUTRAL_COLORS = ['slate', 'gray', 'zinc', 'neutral', 'stone', 'taupe', 'mauve', 'mist', 'olive']
export const PRIMARY_COLORS = ['red', 'orange', 'amber', 'yellow', 'lime', 'green', 'emerald', 'teal', 'cyan', 'sky', 'blue', 'indigo', 'violet', 'purple', 'fuchsia', 'pink', 'rose']
export const RADIUSES = [0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 1]
export const FONT_SIZES = [14, 15, 16, 17, 18]
/** Every font a theme or a profile may use, bundled into the window at build time (nuxt.config.ts). */
export const FONTS = ['Public Sans', 'Geist', 'Geist Mono', 'Inter', 'DM Sans', 'Roboto', 'Figtree', 'Nunito', 'Manrope', 'Plus Jakarta Sans', 'Bricolage Grotesque', 'Outfit', 'Poppins', 'Source Serif 4']
export const ICON_SETS: Record<string, string> = { lucide: 'Lucide', bootstrap: 'Bootstrap', phosphor: 'Phosphor', tabler: 'Tabler', remix: 'Remix', material: 'Material', heroicons: 'Heroicons', iconoir: 'Iconoir' }
export const GROUP_VARIANTS: Record<VariantGroup, string[]> = {
  buttons: ['solid', 'outline', 'soft', 'subtle', 'ghost', 'link'],
  panels: ['solid', 'outline', 'soft', 'subtle'],
  inputs: ['outline', 'soft', 'subtle', 'ghost', 'none']
}
export const SIZES = ['xs', 'sm', 'md', 'lg', 'xl']

/** Each theme's glyph and its main colour in light and dark, for its card (from Nuxt UI's studio and its images). */
export const PRESET_LOOKS: Record<string, { icon: string, light: string, dark: string }> = {
  haunt: { icon: 'i-lucide-git-branch', light: '#000000', dark: '#ffffff' },
  default: { icon: 'i-lucide-mountain', light: '#00C16A', dark: '#00DC82' },
  mono: { icon: 'i-lucide-contrast', light: '#000000', dark: '#ffffff' },
  cobalt: { icon: 'i-lucide-gem', light: '#0d6efd', dark: '#3d8bfd' },
  sky: { icon: 'i-lucide-cloud-sun', light: '#00a6f4', dark: '#00bcff' },
  mint: { icon: 'i-lucide-leaf', light: '#00bba7', dark: '#00d5be' },
  iris: { icon: 'i-lucide-flower', light: '#8e51ff', dark: '#a684ff' },
  crimson: { icon: 'i-lucide-clapperboard', light: '#fb2c36', dark: '#ff6467' },
  coral: { icon: 'i-lucide-shell', light: '#ff2056', dark: '#ff637e' },
  sunset: { icon: 'i-lucide-sunset', light: '#ff6900', dark: '#ff8904' },
  carbon: { icon: 'i-lucide-zap', light: '#fe9a00', dark: '#ffb900' },
  bubblegum: { icon: 'i-lucide-candy', light: '#f6339a', dark: '#fb64b6' },
  parchment: { icon: 'i-lucide-scroll-text', light: '#d97757', dark: '#dd9977' }
}

/**
 * The five tokens the library pins to white, routed through the neutral ramp
 * the way picking a neutral in the studio does (selectPalette's remaps), so a
 * tinted ramp reaches the page and the export reproduces the preview.
 *
 * The page moving to 50 takes the muted surface with it: the library sits it
 * at 50 too, so it would land ON the page and every muted panel (a code
 * block, a table head) would lose its shape.
 */
const tintedNeutralBase = {
  '--ui-bg': { light: 50 },
  '--ui-bg-muted': { light: 100 },
  '--ui-text-inverted': { light: 50 },
  '--ui-text-highlighted': { dark: 50 },
  '--ui-bg-inverted': { dark: 50 },
  '--ui-border-inverted': { dark: 50 }
} satisfies ThemeStyle['tokenShades']

/**
 * A fresh copy per preset. `deriveStyle` puts these objects straight into
 * reactive studio state, so a spread would hand six presets the same
 * per-token object and one in-place write would corrupt all of them.
 */
const tintedNeutral = () => structuredClone(tintedNeutralBase)

/**
 * Presets are plain ThemeDocs: applying one replaces the current document.
 */
export const presets: ThemePreset[] = [{
  id: 'haunt',
  name: 'Haunt',
  description: "Haunt's own: black or white on pure gray, the system's fonts, Lucide icons.",
  doc: {
    version: 1,
    blackAsPrimary: true,
    colors: { neutral: 'neutral' },
    radius: 0.5,
    font: { sans: 'system' }
  }
}, {
  id: 'default',
  name: 'Default',
  description: "Nuxt UI's stock theme: green on slate, Public Sans.",
  doc: {
    version: 1
  }
}, {
  id: 'mono',
  name: 'Mono',
  description: 'Black on a pure gray neutral, generous radius, quiet surfaces.',
  doc: {
    version: 1,
    blackAsPrimary: true,
    colors: {
      neutral: 'neutral'
    },
    radius: 0.5,
    font: { sans: 'Geist', mono: 'Geist Mono' },
    style: {
      defaults: {
        variants: {
          inputs: 'subtle'
        }
      },
      tokenShades: {
        '--ui-bg': { dark: 950 },
        '--ui-bg-muted': { light: 100, dark: 900 },
        '--ui-bg-elevated': { light: 100, dark: 900 },
        '--ui-bg-accented': { light: 200, dark: 800 },
        '--ui-text': { light: 900, dark: 100 },
        '--ui-text-highlighted': { light: 950, dark: 50 }
      }
    }
  }
}, {
  id: 'cobalt',
  name: 'Cobalt',
  description: 'Utility blue on cool grays, tight corners, flat bordered surfaces.',
  doc: {
    version: 1,
    // Prefixed names: a palette named plainly 'blue'/'gray' would override
    // the same-named tailwind ramp app-wide while active.
    palettes: {
      'cobalt': {
        shades: {
          50: 'oklch(95.3% 0.022 260.723)',
          100: 'oklch(90.8% 0.045 258.763)',
          200: 'oklch(81.6% 0.091 257.776)',
          300: 'oklch(72.9% 0.14 258.068)',
          400: 'oklch(64.7% 0.186 258.256)',
          500: 'oklch(57.8% 0.228 260.025)',
          600: 'oklch(49.2% 0.19 259.799)',
          700: 'oklch(40.2% 0.152 259.656)',
          800: 'oklch(30.7% 0.109 258.934)',
          900: 'oklch(20.4% 0.063 257.52)',
          950: 'oklch(14.7% 0.037 249.929)'
        }
      },
      // A flat cool gray, its mid stops lighter than tailwind's so borders
      // and muted text stay soft against the tinted page.
      'cobalt-gray': {
        shades: {
          50: 'oklch(99.1% 0 0)',
          100: 'oklch(98.2% 0.002 247.839)',
          200: 'oklch(94.2% 0.005 247.879)',
          300: 'oklch(91.1% 0.007 247.901)',
          400: 'oklch(86.7% 0.011 247.949)',
          500: 'oklch(76.9% 0.015 248.017)',
          600: 'oklch(55.8% 0.016 244.893)',
          700: 'oklch(42.8% 0.015 248.172)',
          800: 'oklch(34.5% 0.013 248.212)',
          900: 'oklch(26.2% 0.009 248.19)',
          950: 'oklch(20.7% 0.008 248.192)'
        }
      }
    },
    // The gray ramp serves as secondary too: a muted second button colour.
    colors: {
      primary: 'cobalt',
      secondary: 'cobalt-gray',
      info: 'cyan',
      warning: 'amber',
      neutral: 'cobalt-gray'
    },
    radius: 0.125,
    // a point under stock, utility UIs run dense
    fontSize: 15,
    font: { sans: 'Roboto' },
    icons: 'bootstrap',
    // Light rides the gray ramp one step deeper than stock.
    tokens: {
      light: {
        '--ui-secondary': 'var(--ui-color-secondary-600)',
        '--ui-bg-muted': 'var(--ui-color-neutral-200)',
        '--ui-text': 'var(--ui-color-neutral-800)',
        '--ui-border': 'var(--ui-color-neutral-400)',
        '--ui-border-muted': 'var(--ui-color-neutral-300)',
        '--ui-bg': 'var(--ui-color-neutral-100)',
        '--ui-text-toned': 'var(--ui-color-neutral-700)',
        '--ui-text-muted': 'var(--ui-color-neutral-600)',
        '--ui-text-dimmed': 'var(--ui-color-neutral-600)',
        '--ui-bg-elevated': 'var(--ui-color-neutral-200)',
        '--ui-bg-accented': 'var(--ui-color-neutral-300)'
      },
      // Dark keeps the same brand colours rather than lifting to the 400s.
      dark: {
        '--ui-primary': 'var(--ui-color-primary-500)',
        '--ui-secondary': 'var(--ui-color-secondary-500)',
        '--ui-success': 'var(--ui-color-success-500)',
        '--ui-info': 'var(--ui-color-info-500)',
        '--ui-warning': 'var(--ui-color-warning-500)',
        '--ui-error': 'var(--ui-color-error-500)',
        '--ui-bg-muted': 'var(--ui-color-neutral-700)',
        '--ui-text': 'var(--ui-color-neutral-300)'
      }
    }
  }
}, {
  id: 'sky',
  name: 'Sky',
  description: 'Sky blue on a mist neutral, pastel fills everywhere, airy type.',
  doc: {
    version: 1,
    colors: {
      primary: 'sky',
      neutral: 'mist'
    },
    radius: 0.75,
    font: { sans: 'Figtree', lineHeight: 1.6 },
    style: {
      // nothing solid anywhere: one app-wide pastel fill
      defaults: { variants: { buttons: 'soft', inputs: 'soft' } },
      tokenShades: tintedNeutral()
    }
  }
}, {
  id: 'mint',
  name: 'Mint',
  description: 'Teal on an olive neutral, pill buttons and fields, chunky rounded type, large controls.',
  doc: {
    version: 1,
    colors: {
      primary: 'teal',
      neutral: 'olive'
    },
    radius: 0.75,
    // a step heavier across the ladder, Nunito reads thin at stock weights
    font: { sans: 'Nunito', weights: { normal: 500, medium: 600, semibold: 700, bold: 800 } },
    icons: 'iconoir',
    style: {
      defaults: { size: 'lg', variants: { inputs: 'soft' } },
      tokenShades: tintedNeutral()
    },
    // the radius ladder stops at 0.75rem, the pill has to come from the slots
    components: {
      button: { slots: { base: 'rounded-full' } },
      input: { slots: { base: 'rounded-full' } },
      select: { slots: { base: 'rounded-full' } },
      selectMenu: { slots: { base: 'rounded-full' } },
      inputMenu: { slots: { base: 'rounded-full' } }
    }
  }
}, {
  id: 'iris',
  name: 'Iris',
  description: 'Violet outlines on a mauve neutral, fuchsia secondary, tinted fields.',
  doc: {
    version: 1,
    colors: {
      primary: 'violet',
      secondary: 'fuchsia',
      neutral: 'mauve'
    },
    radius: 0.5,
    font: { sans: 'Manrope', letterSpacing: -0.01 },
    icons: 'remix',
    style: {
      // the violet stays a line: outlined actions over tinted fields
      defaults: { variants: { buttons: 'outline', inputs: 'subtle' } },
      tokenShades: tintedNeutral()
    }
  }
}, {
  id: 'crimson',
  name: 'Crimson',
  description: 'Cinema red on pure gray, square corners, filled fields, near-black in dark mode.',
  doc: {
    version: 1,
    colors: {
      primary: 'red',
      neutral: 'neutral'
    },
    radius: 0,
    font: { sans: 'Inter', weights: { semibold: 700, bold: 800 } },
    icons: 'material',
    style: {
      defaults: { variants: { inputs: 'soft' } },
      tokenShades: {
        ...tintedNeutral(),
        // 600 is the deep cinema red, 500 leans orange; dark holds it rather
        // than lifting to the salmon 400
        '--ui-primary': { light: 600, dark: 500 },
        '--ui-bg': { light: 50, dark: 950 },
        // light restates tintedNeutral's step: this key replaces it wholesale
        '--ui-bg-muted': { light: 100, dark: 900 },
        '--ui-bg-elevated': { dark: 900 },
        '--ui-bg-accented': { dark: 800 }
      }
    }
  }
}, {
  id: 'coral',
  name: 'Coral',
  description: 'Rose on warm stone, cards floating on shadows, teal for success, neutral focus rings.',
  doc: {
    version: 1,
    colors: {
      primary: 'rose',
      success: 'teal',
      neutral: 'stone'
    },
    radius: 0.5,
    font: { sans: 'Plus Jakarta Sans' },
    icons: 'phosphor',
    style: {
      // fields ring in neutral, the rose stays on actions
      defaults: { colors: { inputs: 'neutral' } },
      tokenShades: tintedNeutral()
    },
    components: {
      // Cards float on a shadow instead of sitting in a ring. Dark keeps the
      // ring, a shadow has nothing to fall on there. A compound entry rather
      // than the slot: the variant's own `ring` lands after the slot string
      // and would win the merge.
      card: { compoundVariants: [{ variant: 'outline', class: { root: 'ring-0 dark:ring shadow-xl shadow-black/5' } }] }
    }
  }
}, {
  id: 'sunset',
  name: 'Sunset',
  description: 'Orange on a warm taupe neutral, glowing actions, yellow secondary, subtle panels.',
  doc: {
    version: 1,
    colors: {
      primary: 'orange',
      secondary: 'yellow',
      neutral: 'taupe'
    },
    radius: 0.625,
    font: { sans: 'Bricolage Grotesque' },
    icons: 'tabler',
    style: {
      defaults: { variants: { panels: 'subtle' } },
      tokenShades: {
        ...tintedNeutral(),
        // orange-500 is too light to carry white text, 600 is the burnt stop
        '--ui-primary': { light: 600 }
      }
    },
    components: {
      // solid primary actions cast a warm glow
      button: { compoundVariants: [{ color: 'primary', variant: 'solid', class: 'shadow-md shadow-primary/30' }] }
    }
  }
}, {
  id: 'carbon',
  name: 'Carbon',
  description: 'Amber on a warm carbon neutral, with ink-dark borders throughout.',
  doc: {
    version: 1,
    // Sculpted warm neutral: lilac-tinted paper into pure carbon.
    palettes: {
      carbon: {
        shades: {
          50: 'oklch(98.5% 0.017 447.457)',
          100: 'oklch(95.9% 0.036 438.639)',
          200: 'oklch(92.6% 0.054 428.8)',
          300: 'oklch(87% 0.053 417.734)',
          400: 'oklch(70.5% 0.032 405.184)',
          500: 'oklch(55.3% 0.014 390.836)',
          600: 'oklch(44.7% 0.001 374.343)',
          700: 'oklch(35.9% 0 0)',
          800: 'oklch(28% 0 0)',
          900: 'oklch(20.8% 0 0)',
          950: 'oklch(14.1% 0.005 285.805)'
        }
      }
    },
    colors: {
      primary: 'amber',
      secondary: 'yellow',
      neutral: 'carbon'
    },
    radius: 0.5,
    font: { sans: 'Outfit' },
    icons: 'tabler',
    style: {
      defaults: { variants: { buttons: 'solid', panels: 'subtle', inputs: 'subtle' } },
      tokenShades: {
        '--ui-bg': { light: 50, dark: 800 },
        '--ui-bg-muted': { light: 300, dark: 700 },
        '--ui-bg-elevated': { light: 300, dark: 700 },
        '--ui-bg-accented': { light: 400, dark: 600 },
        '--ui-bg-inverted': { light: 900, dark: 50 },
        '--ui-text-inverted': { light: 50 },
        '--ui-text-dimmed': { light: 500, dark: 400 },
        '--ui-text-muted': { light: 800, dark: 300 },
        '--ui-text-toned': { light: 900 },
        '--ui-text': { light: 900 },
        '--ui-text-highlighted': { light: 950, dark: 100 },
        // Light keeps the ink hairline; dark has to run the other way, a
        // border below the surface's own lightness just reads as a seam.
        '--ui-border': { light: 950, dark: 600 },
        // dark is left at the library's own neutral-700, restating it would
        // only add a line the importer then has to recognise as generated
        '--ui-border-muted': { light: 400 },
        '--ui-border-accented': { light: 950, dark: 500 },
        '--ui-border-inverted': { light: 500, dark: 50 }
      }
    }
  }
}, {
  id: 'bubblegum',
  name: 'Bubblegum',
  description: 'Pastel pink softness with mauve-tinted grays.',
  doc: {
    version: 1,
    // Sculpted pink-mauve neutral, chroma peaks mid-ramp.
    palettes: {
      'saturated-mauve': {
        shades: {
          50: 'oklch(96.1% 0.021 325.68)',
          100: 'oklch(92.4% 0.039 325.829)',
          200: 'oklch(88% 0.057 325.83)',
          300: 'oklch(81.2% 0.073 325.398)',
          400: 'oklch(66.1% 0.084 323.292)',
          500: 'oklch(52.4% 0.079 322.443)',
          600: 'oklch(42.7% 0.064 322.128)',
          700: 'oklch(34.6% 0.048 322.004)',
          800: 'oklch(27.4% 0.034 321.983)',
          900: 'oklch(20.7% 0.02 322.028)',
          950: 'oklch(14.5% 0.008 322.12)'
        }
      }
    },
    colors: {
      primary: 'pink',
      secondary: 'violet',
      neutral: 'saturated-mauve'
    },
    radius: 0.375,
    font: { sans: 'Poppins' },
    icons: 'phosphor',
    tokens: {
      light: {
        '--ui-bg': 'var(--ui-color-neutral-50)',
        '--ui-text-inverted': 'var(--ui-color-neutral-50)',
        '--ui-bg-muted': 'var(--ui-color-neutral-100)'
      },
      dark: {
        '--ui-bg-inverted': 'var(--ui-color-neutral-50)',
        '--ui-text-highlighted': 'var(--ui-color-neutral-50)',
        '--ui-border-inverted': 'var(--ui-color-neutral-50)'
      }
    }
  }
}, {
  id: 'parchment',
  name: 'Parchment',
  description: 'Warm parchment neutrals with a book-cloth clay primary.',
  doc: {
    version: 1,
    palettes: {
      // Book-cloth clay: the 500 anchors the ramp, the rest is fitted around it.
      clay: {
        shades: {
          50: 'oklch(97.4% 0.009 48.308)',
          100: 'oklch(94.2% 0.019 52.207)',
          200: 'oklch(88.5% 0.036 51.142)',
          300: 'oklch(82.1% 0.058 50.392)',
          400: 'oklch(74.1% 0.094 47.255)',
          500: 'oklch(67.2% 0.131 38.798)',
          600: 'oklch(58.9% 0.138 37.63)',
          700: 'oklch(51.4% 0.123 37.45)',
          800: 'oklch(44.4% 0.103 36.916)',
          900: 'oklch(38.8% 0.086 36.46)',
          950: 'oklch(25.9% 0.054 38.197)'
        }
      },
      // Warm paper grays, the light end is cream rather than white: the page
      // sits at 100, its surfaces a step below.
      parchment: {
        shades: {
          50: 'oklch(98% 0.006 100)',
          100: 'oklch(96.5% 0.011 99)',
          200: 'oklch(93.6% 0.014 97.348)',
          300: 'oklch(85.8% 0.018 100)',
          400: 'oklch(72.1% 0.015 102.54)',
          500: 'oklch(57.8% 0.008 88.877)',
          600: 'oklch(43.2% 0.006 91.526)',
          700: 'oklch(38.2% 0.003 84.572)',
          800: 'oklch(29.3% 0.003 106.588)',
          900: 'oklch(21.7% 0.002 106.561)',
          950: 'oklch(14.6% 0 0)'
        }
      }
    },
    colors: {
      primary: 'clay',
      neutral: 'parchment'
    },
    radius: 0.375,
    font: { sans: 'DM Sans', serif: 'Source Serif 4' },
    icons: 'heroicons',
    // Surfaces and borders stepped one deeper to hold on the tinted cream
    // page. Elevated has to stay below the page: the library's hover and
    // highlight tints are bg-elevated at half opacity, a lighter elevated
    // vanishes into it.
    tokens: {
      light: {
        '--ui-bg': 'var(--ui-color-neutral-100)',
        '--ui-bg-muted': 'var(--ui-color-neutral-200)',
        '--ui-bg-elevated': 'var(--ui-color-neutral-200)',
        '--ui-bg-accented': 'var(--ui-color-neutral-300)',
        '--ui-border': 'var(--ui-color-neutral-300)',
        // the library leaves this at 200, where the muted surface now sits:
        // a code block's frame would land on its own background
        '--ui-border-muted': 'var(--ui-color-neutral-300)',
        '--ui-border-accented': 'var(--ui-color-neutral-400)'
      },
      dark: {
        '--ui-primary': 'var(--ui-color-primary-500)',
        '--ui-bg-accented': 'var(--ui-color-neutral-800)'
      }
    }
  }
}] satisfies ThemePreset[]
