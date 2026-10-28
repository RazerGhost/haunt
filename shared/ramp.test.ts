import assert from 'node:assert/strict'
import { test } from 'node:test'
import { contrast, hexOf, oklchOf, rampFrom, rgbOf, STEPS } from './ramp.ts'
import { applyLook } from './theme.ts'

const lightness = (hex: string) => oklchOf(rgbOf(hex))[0]

test('a hex is read in either length, anything else is not one', () => {
  assert.equal(hexOf('#2B7FFF'), '#2b7fff')
  assert.equal(hexOf(' #abc '), '#aabbcc')
  assert.equal(hexOf('blue'), null)
  assert.equal(hexOf('#12345'), null)
})

test('Tailwind\'s blue 500 stays itself, and its shades come out near Tailwind\'s own', () => {
  const ramp = rampFrom('#2b7fff')
  assert.equal(ramp[500], '#2b7fff')
  // Tailwind 4's blue: 50 is oklch(97% ...), 950 oklch(28.2% ...).
  assert.ok(Math.abs(lightness(ramp[50]) - 0.97) < 0.01)
  assert.ok(Math.abs(lightness(ramp[950]) - 0.282) < 0.03)
  for (let index = 1; index < STEPS.length; index++) {
    assert.ok(lightness(ramp[STEPS[index]!]) < lightness(ramp[STEPS[index - 1]!]), `${STEPS[index]} is darker than the step before`)
  }
})

test('a very light or dark colour keeps its place and the rest stays in order', () => {
  for (const hex of ['#d0bcff', '#141218', '#ffd60a', '#0a0a0a', '#ffffff', '#808080']) {
    const ramp = rampFrom(hex)
    assert.ok(Object.values(ramp).includes(hex), `${hex} is one of its shades`)
    for (let index = 1; index < STEPS.length; index++) {
      assert.ok(lightness(ramp[STEPS[index]!]) <= lightness(ramp[STEPS[index - 1]!]) + 1e-9, `${hex}: ${STEPS[index]} not lighter`)
    }
  }
})

test('greys from any colour keep text readable where Nuxt UI puts it', () => {
  // Nuxt UI's text on its page: neutral 700 on white (light), neutral 200 on neutral 900 (dark); muted 500 and 400.
  for (const hex of ['#62748e', '#4a3f6b', '#7c6f4f', '#141218', '#cac4d0']) {
    const ramp = rampFrom(hex)
    assert.ok(contrast(ramp[700], '#ffffff') >= 4.5, `${hex}: text on light`)
    assert.ok(contrast(ramp[200], ramp[900]) >= 4.5, `${hex}: text on dark`)
    assert.ok(contrast(ramp[500], '#ffffff') >= 3, `${hex}: muted on light`)
    assert.ok(contrast(ramp[400], ramp[900]) >= 3, `${hex}: muted on dark`)
  }
})

test('a look with a hex gets its shades; a palette name stays as it was', () => {
  const named = applyLook({ primary: 'blue', neutral: 'zinc' })
  assert.equal(named.colors.light.primary, 'blue')
  assert.equal(named.colors.light.neutral, 'zinc')
  assert.doesNotMatch(named.css, /--color-hex-/)

  const hex = applyLook({ primary: '#D0BCFF', darkNeutral: '#141218' })
  assert.equal(hex.colors.light.primary, 'hex-d0bcff')
  assert.equal(hex.colors.dark.neutral, 'hex-141218')
  assert.match(hex.css, /--color-hex-d0bcff-\d+: #d0bcff;/)
  assert.match(hex.css, /--color-hex-141218-50: #[0-9a-f]{6};/)
})

test("the desktop's accent as the main colour, and the theme's without one", () => {
  assert.equal(applyLook({ primary: 'system' }, '#3584e4').colors.light.primary, 'hex-3584e4')
  assert.equal(applyLook({ theme: 'cobalt', darkPrimary: 'system' }, null).colors.dark.primary, 'cobalt')
})

test('a theme file’s roles set Nuxt UI’s tokens exactly, and its opacity the background', () => {
  const applied = applyLook({ roles: { background: '#141218', raised: '#211f26', text: '#e6e0e9', primary: '#d0bcff' }, opacity: 0.85 })
  assert.match(applied.css, /html:root\.dark \{[^}]*--ui-bg-elevated: #211f26;/)
  assert.match(applied.css, /--ui-text-highlighted: #e6e0e9;/)
  assert.match(applied.css, /--ui-primary: #d0bcff;/)
  assert.match(applied.css, /--ui-bg-muted: color-mix\(in oklab, #211f26 50%, #141218\);/)
  assert.match(applied.css, /--ui-bg: color-mix\(in srgb, #141218 85%, transparent\);/)
  assert.doesNotMatch(applied.css, /--ui-text-dimmed/, 'a mix needs both its roles')
})

test('a hex main colour is used as it is where it reads on the page, else the shade beside it', () => {
  const css = applyLook({ primary: '#d0bcff', darkPrimary: '#d0bcff' }).css
  assert.match(css, /html:root\.dark \{[^}]*--ui-primary: #d0bcff;/)
  assert.doesNotMatch(css, /html:root:not\(\.dark\) \{[^}]*--ui-primary: #d0bcff;/, 'too light on white')
})
