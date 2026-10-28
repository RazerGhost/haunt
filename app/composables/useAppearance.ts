import type { Appearance, ThemeFile } from '~/bindings'
import iconSets from '~~/shared/iconSets.json'
import { downloadedIconSets, iconComponent } from './useIconSets'
import { applyLook, type Look } from '~~/shared/theme'

/*
 * How the window looks, per profile (src-tauri/src/settings.rs keeps it with each profile): a theme with the profile's
 * own choices on top (shared/theme.ts). Applied whenever the profile, its look or the window's light or dark changes:
 * Nuxt UI's colours for the mode shown, one stylesheet (ramps, tokens per mode, corners, fonts), the icon set when it is
 * downloaded (useIconSets; as aliases of the Lucide names the code uses), the components' default variants and overrides, and the base font size
 * the text size in Settings scales from. Fonts are not bundled: a theme's font is used when it is installed on the
 * computer, else the system's, or loaded from coolLabs Fonts (fonts.coollabs.io, Google Fonts' fonts without the tracking) while Settings says so (the only time a look goes online;
 * the window's CSP allows those two addresses for fonts only). A profile from before looks were kept takes the light or
 * dark the window had chosen itself. On Linux "system" means the desktop's light or dark, and a main colour can be the
 * desktop's accent (desktop.rs). theme.json (theme_file.rs) goes over the profile's look: its fields, the colours of
 * the window's parts, and a see-through background.
 */

/** The theme's root font size in px; Settings' text size is a step either side of it. */
const baseSize = ref(16)
/** The components the last look changed, to put back before the next one. */
let changed: string[] = []

export function startAppearance(): void {
  const { info } = repositories
  const colorMode = useColorMode()
  const appConfig = useAppConfig()
  const style = document.createElement('style')
  style.id = 'haunt-theme'
  document.head.append(style)
  const { remoteFonts } = usePreferences()
  const fonts = ref<string[]>([])
  // The look's fonts from coolLabs Fonts, while Settings says so.
  watch([fonts, remoteFonts], ([families, remote]) => {
    document.getElementById('haunt-fonts')?.remove()
    if (!remote || !families.length) return
    const link = document.createElement('link')
    link.id = 'haunt-fonts'
    link.rel = 'stylesheet'
    link.href = `https://api.fonts.coollabs.io/css2?${families.map(name => `family=${name.replace(/ /g, '+')}:wght@400;500;600;700`).join('&')}&display=swap`
    document.head.append(link)
  })

  const downloaded = downloadedIconSets()
  const { desktop, themeFile, transparent } = repositories
  watch([() => info.value?.appearance, () => colorMode.value, downloaded, desktop, themeFile, transparent], ([own]) => {
    if (!own) return
    const look = withThemeFile(own, themeFile.value.theme, transparent.value)
    if (!look.mode) {
      // Kept by the window until now: the profile takes it from here.
      void haunt.setAppearance({ ...own, mode: colorMode.preference }).then((next) => { info.value = next }).catch(() => {})
      return
    }
    // "System" is the desktop's light or dark where Haunt can read it (Linux: the webview's own follows GTK alone).
    const system = desktop.value.dark === null ? 'system' : desktop.value.dark ? 'dark' : 'light'
    colorMode.preference = look.mode === 'system' ? system : look.mode
    const applied = applyLook(look, desktop.value.accent)
    const dark = colorMode.value === 'dark'
    // Nuxt UI's app config types its colours as the ones configured; any colour name works at runtime.
    Object.assign(appConfig.ui.colors as Record<string, string>, dark ? applied.colors.dark : applied.colors.light)
    style.textContent = applied.css
    baseSize.value = applied.fontSize
    if (applied.fonts.join() !== fonts.value.join()) fonts.value = applied.fonts
    const icon = appConfig.icon as { aliases?: Record<string, string> }
    const map = (iconSets.sets as Record<string, Record<string, string>>)[applied.icons]
    icon.aliases = map && downloaded.value.has(applied.icons)
      ? Object.fromEntries(Object.entries(map).map(([lucide, target]) => [lucide, iconComponent(applied.icons, target.split(':')[1]!)]))
      : {}
    const ui = appConfig.ui as Record<string, unknown>
    for (const name of changed) delete ui[name]
    for (const [name, config] of Object.entries(applied.components)) ui[name] = config
    changed = Object.keys(applied.components)
  }, { immediate: true })
}

/** The profile's look with what theme.json sets on top (each field it leaves empty is the profile's). */
function withThemeFile(own: Appearance, file: ThemeFile | null, transparent: boolean): Look {
  if (!file) return own
  const { roles, opacity, ...fields } = file
  const set = Object.fromEntries(Object.entries(fields).filter(([, value]) => value))
  return { ...own, ...set, roles: Object.fromEntries(Object.entries(roles ?? {}).filter(([, value]) => value)), opacity: transparent ? opacity : null }
}

/** The fields of Appearance theme.json sets, which the profile's own choices do not change while it does. */
export function themeFileFields(file: ThemeFile | null): Set<string> {
  return new Set(Object.entries(file ?? {}).filter(([key, value]) => key !== 'roles' && key !== 'opacity' && value).map(([key]) => key))
}

export function useAppearance() {
  const { info } = repositories
  const failure = useFailure()
  const look = computed(() => info.value?.appearance ?? null)
  /** Changes the look of the profile in use. */
  async function change(next: Partial<Appearance>): Promise<void> {
    if (!look.value) return
    try {
      info.value = await haunt.setAppearance({ ...look.value, ...next })
    } catch (error) {
      failure.failed(t('composables.useAppearance.notChanged'), error, t('common.tryAgain'))
    }
  }
  /** A theme, with every choice of the profile's own put back to the theme's. */
  async function pickTheme(theme: string): Promise<void> {
    if (!look.value) return
    const cleared = Object.fromEntries(Object.keys(look.value).filter(key => key !== 'mode').map(key => [key, ''])) as Partial<Appearance>
    await change({ ...cleared, theme })
  }
  return { look, change, pickTheme, baseSize }
}
