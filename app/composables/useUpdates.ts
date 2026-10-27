/*
 * Looking for a new version of Haunt (GitHub's releases of razerghost/haunt): only when switched on in Settings (About),
 * then at most once a day, or once when you press Check for updates, whatever the switch says. Nothing else is sent.
 * One copy for the app; plugins/haunt.client.ts starts it.
 */
import { compareVersions } from '~~/shared/versions'

const RELEASES = 'https://api.github.com/repos/razerghost/haunt/releases/latest'
const DAY_MS = 24 * 60 * 60 * 1000

export interface UpdateResult { latest: string | null, url: string | null, newer: boolean, problem: string | null }

const result = shallowRef<UpdateResult | null>(null)
const checking = ref(false)
const lastCheck = usePersisted<number>('update-checked', 0, value => typeof value === 'number')

async function check(current: string): Promise<UpdateResult> {
  checking.value = true
  try {
    const response = await fetch(RELEASES, { headers: { Accept: 'application/vnd.github+json' } })
    lastCheck.value = Date.now()
    if (response.status === 404) return (result.value = { latest: null, url: null, newer: false, problem: t('composables.useUpdates.noRelease') })
    if (!response.ok) return (result.value = { latest: null, url: null, newer: false, problem: t('composables.useUpdates.answered', { status: response.status }) })
    const release = await response.json() as { tag_name?: string, html_url?: string }
    const latest = release.tag_name ?? null
    return (result.value = { latest, url: release.html_url ?? null, newer: !!latest && compareVersions(latest, current) > 0, problem: null })
  } catch {
    return (result.value = { latest: null, url: null, newer: false, problem: t('composables.useUpdates.unreachable') })
  } finally {
    checking.value = false
  }
}

export function startUpdates(): void {
  void haunt.appSettings().then(async (settings) => {
    if (!settings.updateCheck || Date.now() - lastCheck.value < DAY_MS) return
    const found = await check(settings.version)
    if (found.newer) notifyNewer(found)
  }).catch(() => {})
}

function notifyNewer(found: UpdateResult): void {
  useToast().add({
    title: t('composables.useUpdates.newer', { version: found.latest ?? '' }),
    description: t('composables.useUpdates.newerHelp'),
    icon: 'i-lucide-sparkles',
    actions: found.url ? [{ label: t('composables.useUpdates.seeIt'), color: 'neutral', variant: 'outline', onClick: () => { void haunt.openRelease(found.url!) } }] : undefined
  })
}

export function useUpdates() {
  return { result, checking, lastCheck, check, open: (url: string) => haunt.openRelease(url) }
}
