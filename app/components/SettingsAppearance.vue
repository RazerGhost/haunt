<script setup lang="ts">
import { themeOf } from '~~/shared/theme'
import { FONT_SIZES, FONTS, GROUP_VARIANTS, ICON_SETS, NEUTRAL_COLORS, PRESET_LOOKS, PRIMARY_COLORS, RADIUSES, SIZES, STOCK, presets } from '~~/shared/themes'

/*
 * Settings, Appearance: the profile's look (a theme from shared/themes.ts, light or dark, Nuxt UI's main and neutral
 * colours, the corners, fonts and icons), kept by Rust with each profile (useAppearance). The profile's own choices go
 * on top of the theme, each "Theme's" when empty.
 */
const { remoteFonts } = usePreferences()
const appearance = useAppearance()
const { info } = useRepositories()

const themes = [
  { value: 'system', icon: 'i-lucide-monitor' },
  { value: 'light', icon: 'i-lucide-sun' },
  { value: 'dark', icon: 'i-lucide-moon' }
]
const PRIMARY: Record<string, string> = {
  red: '#fb2c36', orange: '#ff6900', amber: '#fe9a00', yellow: '#f0b100', lime: '#7ccf00', green: '#00c950', emerald: '#00bc7d',
  teal: '#00bba7', cyan: '#00b8db', sky: '#00a6f4', blue: '#2b7fff', indigo: '#615fff', violet: '#8e51ff', purple: '#ad46ff',
  fuchsia: '#e12afb', pink: '#f6339a', rose: '#ff2056'
}
const NEUTRAL: Record<string, string> = {
  slate: '#62748e', gray: '#6a7282', zinc: '#71717b', neutral: '#737373', stone: '#79716b',
  taupe: 'oklch(54.7% 0.021 43.1)', mauve: 'oklch(54.2% 0.034 322.5)', mist: 'oklch(56% 0.021 213.5)', olive: 'oklch(58% 0.031 107.3)'
}
/** The colours for each mode: Nuxt UI keeps one set, swapped as the window turns light or dark (useAppearance). */
const PALETTES = [
  { mode: 'light', icon: 'i-lucide-sun', primary: 'primary', neutral: 'neutral', plain: '#000' },
  { mode: 'dark', icon: 'i-lucide-moon', primary: 'darkPrimary', neutral: 'darkNeutral', plain: '#fff' }
] as const
const THEMES = presets.map(preset => ({ ...preset, look: PRESET_LOOKS[preset.id]! }))
const chosenTheme = computed(() => themeOf(appearance.look.value?.theme))
type ThemeDoc = (typeof presets)[number]['doc']
const themeFont = (doc: ThemeDoc) => (doc.font?.sans === 'system' ? t('SettingsAppearance.systemFonts') : doc.font?.sans ?? STOCK.font)
const colorMode = useColorMode()
const chipColor = (look: { light: string, dark: string }) => (colorMode.value === 'dark' ? look.dark : look.light)
/** The profile's own choices beside the theme, for "Back to the theme". */
const ownChoices = computed(() => Object.entries(appearance.look.value ?? {}).filter(([key, value]) => key !== 'mode' && key !== 'theme' && value).length)
/** A select's value: its "Theme's" entry stands for the empty string, which a select cannot hold. */
const THEME_VALUE = 'theme'
const withTheme = (label: string, values: { value: string, label: string }[]) => [{ value: THEME_VALUE, label: t('SettingsAppearance.themesWith', { label }) }, ...values]
const selected = (value: string | undefined) => value || THEME_VALUE
const choose = (key: string, value: unknown) => appearance.change({ [key]: value === THEME_VALUE ? '' : String(value) })
const SEMANTIC = [{ key: 'secondary' }, { key: 'success' }, { key: 'info' }, { key: 'warning' }, { key: 'error' }] as const
const colorItems = (key: (typeof SEMANTIC)[number]['key']) => withTheme(chosenTheme.value.doc.colors?.[key] ?? STOCK.colors[key],
  [...PRIMARY_COLORS, ...NEUTRAL_COLORS].map(value => ({ value, label: value })))
const radiusItems = computed(() => withTheme(`${chosenTheme.value.doc.radius ?? STOCK.radius} rem`, RADIUSES.map(value => ({ value: String(value), label: `${value} rem` }))))
const fontItems = computed(() => withTheme(themeFont(chosenTheme.value.doc), [{ value: 'system', label: t('SettingsAppearance.systemFonts') }, ...FONTS.map(value => ({ value, label: value }))]))
const sizeItems = computed(() => withTheme(`${chosenTheme.value.doc.fontSize ?? STOCK.fontSize} px`, FONT_SIZES.map(value => ({ value: String(value), label: `${value} px` }))))
const icons = useIconSets()
/** Lucide is bundled; another set shows once it is downloaded (useIconSets). */
const iconReady = (set: string) => set === 'lucide' || icons.downloaded.value.has(set)
const iconItems = computed(() => withTheme(ICON_SETS[chosenTheme.value.doc.icons ?? STOCK.icons] ?? 'Lucide',
  Object.entries(ICON_SETS).map(([value, label]) => ({ value, label: iconReady(value) ? label : t('SettingsAppearance.toDownload', { label }) }))))
/** The icon set the look asks for, and whether it shows (or Lucide does, until it is downloaded). */
const wantedIcons = computed(() => appearance.look.value?.icons || chosenTheme.value.doc.icons || STOCK.icons)
async function chooseIcons(value: unknown): Promise<void> {
  const set = value === THEME_VALUE ? chosenTheme.value.doc.icons ?? STOCK.icons : String(value)
  if (!iconReady(set) && !(await icons.download(set))) return
  await choose('icons', value)
}
const GROUPS = [
  { key: 'buttons', stock: 'solid' }, { key: 'inputs', stock: 'outline' }, { key: 'panels', stock: 'outline' }
] as const
const groupItems = (group: (typeof GROUPS)[number]) => withTheme(chosenTheme.value.doc.style?.defaults?.variants?.[group.key] ?? group.stock,
  GROUP_VARIANTS[group.key].map(value => ({ value, label: value })))
const controlSizes = computed(() => withTheme(chosenTheme.value.doc.style?.defaults?.size ?? 'md', SIZES.map(value => ({ value, label: value }))))
const theme = computed({ get: () => appearance.look.value?.mode || 'system', set: (value: string) => { void appearance.change({ mode: value }) } })
</script>

<template>
  <div class="grid content-start gap-5">
    <p class="text-xs text-muted">{{ t('SettingsAppearance.profileBefore') }} <b class="text-highlighted">{{ info?.profile }}</b> {{ t('SettingsAppearance.profileAfter') }}</p>
    <UFormField :label="t('SettingsAppearance.theme')">
      <div class="grid grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))] gap-1.5">
        <button
          v-for="item in THEMES"
          :key="item.id"
          type="button"
          class="flex items-center gap-2.5 rounded-md border px-2 py-1.5 text-left"
          :class="chosenTheme.id === item.id ? 'border-primary bg-elevated' : 'border-default hover:bg-elevated/60'"
          :title="item.description"
          :aria-pressed="chosenTheme.id === item.id"
          @click="appearance.pickTheme(item.id)"
        >
          <span class="grid size-8 shrink-0 place-items-center rounded-md" :style="{ background: `color-mix(in oklab, ${chipColor(item.look)} 16%, transparent)`, color: chipColor(item.look) }">
            <UIcon :name="item.look.icon" class="size-4" />
          </span>
          <span class="grid min-w-0">
            <span class="truncate text-ui font-medium text-highlighted">{{ item.name }}</span>
            <span class="truncate text-label text-muted">{{ themeFont(item.doc) }} · {{ ICON_SETS[item.doc.icons ?? 'lucide'] }}</span>
          </span>
        </button>
      </div>
    </UFormField>
    <div v-if="icons.downloaded.value.size" class="flex flex-wrap items-center gap-1.5 text-xs text-muted">
      {{ t('SettingsAppearance.iconSetsKept') }}
      <UBadge v-for="set in icons.downloaded.value" :key="set" color="neutral" variant="subtle" size="sm">
        {{ ICON_SETS[set] }}
        <button type="button" class="ml-0.5 hover:text-highlighted" :aria-label="t('SettingsAppearance.removeIcons', { set: ICON_SETS[set]! })" @click="icons.remove(set)"><UIcon name="i-lucide-x" class="size-3" /></button>
      </UBadge>
    </div>
    <div v-if="ownChoices" class="flex items-center gap-2 text-xs text-muted">
      <span class="flex-1">{{ t('SettingsAppearance.ownChoices', { count: ownChoices, theme: chosenTheme.name }) }}</span>
      <UButton :label="t('SettingsAppearance.backToTheme')" icon="i-lucide-rotate-ccw" size="xs" color="neutral" variant="outline" @click="appearance.pickTheme(chosenTheme.id)" />
    </div>
    <UFormField :label="t('SettingsAppearance.lightOrDark')">
      <UFieldGroup>
        <UButton
          v-for="option in themes"
          :key="option.value"
          :label="t(`SettingsAppearance.mode.${option.value}`)"
          :icon="option.icon"
          size="sm"
          :color="theme === option.value ? 'primary' : 'neutral'"
          :variant="theme === option.value ? 'solid' : 'outline'"
          @click="theme = option.value"
        />
      </UFieldGroup>
    </UFormField>
    <section v-for="palette in PALETTES" :key="palette.mode" class="grid gap-3 rounded-md border border-default p-3">
      <h3 class="flex items-center gap-1.5 text-xs font-semibold text-highlighted">
        <UIcon :name="palette.icon" class="size-3.5" />{{ t(`SettingsAppearance.palette.${palette.mode}`) }}
        <UButton v-if="palette.mode === 'dark'" :label="t('SettingsAppearance.sameAsLight')" size="xs" color="neutral" variant="ghost" class="ml-auto" @click="appearance.change({ darkPrimary: appearance.look.value?.primary, darkNeutral: appearance.look.value?.neutral })" />
      </h3>
      <UFormField :label="t('SettingsAppearance.mainColour')" :help="palette.mode === 'light' ? t('SettingsAppearance.mainColourLightHelp') : t('SettingsAppearance.mainColourDarkHelp')">
        <div class="flex max-w-lg flex-wrap items-center gap-1.5">
          <UButton :label="t('SettingsAppearance.themes')" size="xs" :color="!appearance.look.value?.[palette.primary] ? 'primary' : 'neutral'" :variant="!appearance.look.value?.[palette.primary] ? 'solid' : 'outline'" @click="appearance.change({ [palette.primary]: '' })" />
          <button
            v-for="(swatch, name) in { neutral: palette.plain, ...PRIMARY }"
            :key="name"
            type="button"
            class="grid size-7 place-items-center rounded-full border-2"
            :class="appearance.look.value?.[palette.primary] === name ? 'border-highlighted' : 'border-transparent hover:border-accented'"
            :title="name === 'neutral' ? t('SettingsAppearance.plain') : name"
            :aria-label="t('SettingsAppearance.mainColourOf', { palette: t(`SettingsAppearance.palette.${palette.mode}`), colour: name === 'neutral' ? t('SettingsAppearance.plainName') : name })"
            :aria-pressed="appearance.look.value?.[palette.primary] === name"
            @click="appearance.change({ [palette.primary]: name })"
          >
            <span class="size-5 rounded-full border border-default" :style="{ background: swatch }" />
          </button>
        </div>
      </UFormField>
      <UFormField :label="t('SettingsAppearance.greys')" :help="t('SettingsAppearance.greysHelp')">
        <div class="flex max-w-lg flex-wrap gap-1">
          <UButton :label="t('SettingsAppearance.themes')" size="xs" :color="!appearance.look.value?.[palette.neutral] ? 'primary' : 'neutral'" :variant="!appearance.look.value?.[palette.neutral] ? 'solid' : 'outline'" @click="appearance.change({ [palette.neutral]: '' })" />
          <UButton
            v-for="(swatch, name) in NEUTRAL"
            :key="name"
            :label="name"
            size="xs"
            class="capitalize"
            :color="appearance.look.value?.[palette.neutral] === name ? 'primary' : 'neutral'"
            :variant="appearance.look.value?.[palette.neutral] === name ? 'solid' : 'outline'"
            @click="appearance.change({ [palette.neutral]: name })"
          >
            <template #leading><span class="size-3 rounded-full" :style="{ background: swatch }" /></template>
          </UButton>
        </div>
      </UFormField>
    </section>
    <USwitch v-model="remoteFonts" :label="t('SettingsAppearance.webFonts')" :description="t('SettingsAppearance.webFontsHelp')" />
    <UFormField :label="t('SettingsAppearance.otherColours')" :help="t('SettingsAppearance.otherColoursHelp')">
      <div class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2">
        <UFormField v-for="role in SEMANTIC" :key="role.key" :label="t(`SettingsAppearance.role.${role.key}`)" size="xs">
          <USelect :model-value="selected(appearance.look.value?.[role.key])" :items="colorItems(role.key)" size="sm" class="w-full capitalize" @update:model-value="value => choose(role.key, value)" />
        </UFormField>
      </div>
    </UFormField>
    <div class="grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-3">
      <UFormField :label="t('SettingsAppearance.corners')">
        <USelect :model-value="selected(appearance.look.value?.radius)" :items="radiusItems" size="sm" class="w-full" @update:model-value="value => choose('radius', value)" />
      </UFormField>
      <UFormField :label="t('SettingsAppearance.font')" :help="remoteFonts ? t('SettingsAppearance.fontRemoteHelp') : t('SettingsAppearance.fontLocalHelp')">
        <USelect :model-value="selected(appearance.look.value?.font)" :items="fontItems" size="sm" class="w-full" @update:model-value="value => choose('font', value)" />
      </UFormField>
      <UFormField :label="t('SettingsAppearance.fontSize')" :help="t('SettingsAppearance.fontSizeHelp')">
        <USelect :model-value="selected(appearance.look.value?.fontSize)" :items="sizeItems" size="sm" class="w-full" @update:model-value="value => choose('fontSize', value)" />
      </UFormField>
      <UFormField :label="t('SettingsAppearance.icons')" :help="iconReady(wantedIcons) ? t('SettingsAppearance.iconsHelp') : undefined">
        <USelect :model-value="selected(appearance.look.value?.icons)" :items="iconItems" size="sm" class="w-full" :loading="!!icons.downloading.value" @update:model-value="chooseIcons" />
        <template v-if="!iconReady(wantedIcons)" #help>
          <span class="flex flex-wrap items-center gap-1.5">
            {{ t('SettingsAppearance.notDownloaded', { set: ICON_SETS[wantedIcons]! }) }}
            <UButton :label="t('SettingsAppearance.download', { set: ICON_SETS[wantedIcons]! })" icon="i-lucide-cloud-download" size="xs" color="neutral" variant="outline" :loading="icons.downloading.value === wantedIcons" @click="icons.download(wantedIcons)" />
          </span>
        </template>
      </UFormField>
      <UFormField v-for="group in GROUPS" :key="group.key" :label="t(`SettingsAppearance.group.${group.key}`)">
        <USelect :model-value="selected(appearance.look.value?.[group.key])" :items="groupItems(group)" size="sm" class="w-full capitalize" @update:model-value="value => choose(group.key, value)" />
      </UFormField>
      <UFormField :label="t('SettingsAppearance.controlSize')">
        <USelect :model-value="selected(appearance.look.value?.size)" :items="controlSizes" size="sm" class="w-full" @update:model-value="value => choose('size', value)" />
      </UFormField>
    </div>
  </div>
</template>
