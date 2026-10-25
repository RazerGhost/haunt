import assert from 'node:assert/strict'
import { test } from 'node:test'
import { applyLook } from './theme.ts'
import iconSets from './iconSets.json' with { type: 'json' }
import { ICON_SETS, presets } from './themes.ts'

test('Haunt\'s own look: plain black or white on pure gray, system fonts, Lucide', () => {
  const applied = applyLook({})
  assert.deepEqual(applied.plain, { light: true, dark: true })
  assert.equal(applied.colors.light.neutral, 'neutral')
  assert.match(applied.css, /html:root:not\(\.dark\) \{ --ui-primary: black; \}/)
  assert.match(applied.css, /--font-sans: system-ui/)
  assert.equal(applied.icons, 'lucide')
  assert.deepEqual(applied.components, {})
})

test('a theme brings its ramps, tokens, fonts, icons and component defaults', () => {
  const cobalt = applyLook({ theme: 'cobalt' })
  assert.equal(cobalt.colors.light.primary, 'cobalt')
  assert.equal(cobalt.colors.dark.secondary, 'cobalt-gray')
  assert.match(cobalt.css, /--color-cobalt-500: oklch\(57\.8% 0\.228 260\.025\)/)
  assert.match(cobalt.css, /--font-sans: "Roboto"/)
  assert.match(cobalt.css, /--ui-radius: 0\.125rem/)
  assert.equal(cobalt.fontSize, 15)
  assert.equal(cobalt.icons, 'bootstrap')
  assert.deepEqual(cobalt.fonts, ['Roboto'])
  assert.deepEqual(applyLook({ theme: 'parchment' }).fonts, ['DM Sans', 'Source Serif 4'])
  assert.deepEqual(applyLook({}).fonts, [], "Haunt's own uses the system's fonts")

  const sky = applyLook({ theme: 'sky' })
  assert.deepEqual((sky.components.button as { defaultVariants: unknown }).defaultVariants, { variant: 'soft' })
  assert.deepEqual((sky.components.input as { defaultVariants: unknown }).defaultVariants, { variant: 'soft' })
  // tokenShades: the page on the neutral ramp's 50 in light mode.
  assert.match(sky.css, /--ui-bg: var\(--ui-color-neutral-50\)/)

  const mint = applyLook({ theme: 'mint' })
  assert.equal((mint.components.button as { defaultVariants: { size: string } }).defaultVariants.size, 'lg')
  assert.deepEqual((mint.components.button as { slots: unknown }).slots, { base: 'rounded-full' })
})

test('a profile\'s own choices win over the theme, each mode on its own', () => {
  const look = applyLook({ theme: 'crimson', primary: 'blue', darkPrimary: 'neutral', neutral: 'zinc', radius: '0.75', font: 'Geist', icons: 'tabler', buttons: 'outline', fontSize: '17' })
  assert.equal(look.colors.light.primary, 'blue')
  assert.equal(look.colors.light.neutral, 'zinc')
  assert.equal(look.colors.dark.neutral, 'neutral', 'the dark greys stay the theme\'s')
  assert.deepEqual(look.plain, { light: false, dark: true })
  assert.match(look.css, /html:root\.dark \{[^}]*--ui-primary: white;/)
  assert.match(look.css, /--ui-radius: 0\.75rem/)
  assert.match(look.css, /--font-sans: "Geist"/)
  assert.equal(look.icons, 'tabler')
  assert.equal(look.fontSize, 17)
  assert.equal((look.components.button as { defaultVariants: { variant: string } }).defaultVariants.variant, 'outline')
})

test('every theme applies, and its icons are a set Haunt bundles', () => {
  for (const preset of presets) {
    const applied = applyLook({ theme: preset.id })
    assert.ok(ICON_SETS[applied.icons], `${preset.id}: ${applied.icons}`)
    assert.ok(applied.icons === 'lucide' || (iconSets.sets as Record<string, unknown>)[applied.icons], applied.icons)
  }
})
