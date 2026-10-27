import iconSets from '~~/shared/iconSets.json'

/*
 * The icon sets a theme may use besides Lucide (which is bundled): downloaded on request, only the icons Haunt names
 * (shared/iconSets.json), from the Iconify API; checked and kept by Rust (src-tauri/src/icons.rs), and read back at
 * every start, so they work offline. Each icon becomes a small component that draws it (`haunt-icon-<set>-<name>`);
 * Nuxt Icon draws a name that is a component with that component, so a look's aliases point the Lucide names there
 * (useAppearance). A set not downloaded leaves the icons Lucide.
 */

interface IconifyIcon { body: string, width?: number, height?: number, left?: number, top?: number }
interface Collection { prefix: string, icons: Record<string, IconifyIcon>, aliases?: Record<string, { parent: string }>, width?: number, height?: number }

const PREFIX: Record<string, string> = { bootstrap: 'bi', phosphor: 'ph', tabler: 'tabler', remix: 'ri', material: 'material-symbols', heroicons: 'heroicons', iconoir: 'iconoir' }
const downloaded = ref<Set<string>>(new Set())
const downloading = ref<string | null>(null)

/** The sets kept on this computer, for code outside a component (the look, applied from the plugin). */
export function downloadedIconSets() {
  return downloaded
}

/** The component name an icon of a downloaded set is drawn by. */
export function iconComponent(set: string, name: string): string {
  return `haunt-icon-${set}-${name}`
}

function register(set: string, collection: Collection): void {
  const app = useNuxtApp().vueApp
  const draw = (icon: IconifyIcon) => defineComponent({
    props: { name: String, customize: [Function, Boolean, null] },
    setup: () => () => h('svg', {
      'xmlns': 'http://www.w3.org/2000/svg',
      'viewBox': `${icon.left ?? 0} ${icon.top ?? 0} ${icon.width ?? collection.width ?? 16} ${icon.height ?? collection.height ?? 16}`,
      'width': '1em',
      'height': '1em',
      'aria-hidden': 'true',
      // Checked by Rust before it was kept: drawing elements only, nothing that runs or reaches out.
      'innerHTML': icon.body
    })
  })
  for (const [name, icon] of Object.entries(collection.icons)) app.component(iconComponent(set, name), draw(icon))
  for (const [name, alias] of Object.entries(collection.aliases ?? {})) {
    const parent = collection.icons[alias.parent]
    if (parent) app.component(iconComponent(set, name), draw(parent))
  }
}

/** Reads the kept sets; called once at start, before the look is applied. */
export async function startIconSets(): Promise<void> {
  try {
    const sets = await haunt.iconSets()
    for (const set of sets) register(set, JSON.parse(await haunt.iconSet(set)) as Collection)
    downloaded.value = new Set(sets)
  } catch { /* none kept, or unreadable: the icons stay Lucide */ }
}

export function useIconSets() {
  const failure = useFailure()
  const toast = useToast()
  async function download(set: string): Promise<boolean> {
    const prefix = PREFIX[set]
    const map = (iconSets.sets as Record<string, Record<string, string>>)[set]
    if (!prefix || !map || downloading.value) return false
    downloading.value = set
    try {
      const names = [...new Set(Object.values(map).map(target => target.split(':')[1]!))]
      const response = await fetch(`https://api.iconify.design/${prefix}.json?icons=${names.join(',')}`)
      if (!response.ok) throw new Error(`The icon service answered ${response.status}.`)
      await haunt.saveIconSet(set, await response.text())
      register(set, JSON.parse(await haunt.iconSet(set)) as Collection)
      downloaded.value = new Set([...downloaded.value, set])
      toast.add({ title: t('composables.useIconSets.downloaded', { set }), description: t('composables.useIconSets.downloadedHelp'), icon: 'i-lucide-cloud-download', color: 'success' })
      return true
    } catch (error) {
      failure.failed(t('composables.useIconSets.notDownloaded', { set }), error, t('composables.useIconSets.checkConnection'))
      return false
    } finally {
      downloading.value = null
    }
  }
  async function remove(set: string): Promise<void> {
    try {
      await haunt.deleteIconSet(set)
      const next = new Set(downloaded.value)
      next.delete(set)
      downloaded.value = next
    } catch (error) {
      failure.failed(t('composables.useIconSets.notRemoved', { set }), error, t('common.tryAgain'))
    }
  }
  return { downloaded, downloading, download, remove }
}
