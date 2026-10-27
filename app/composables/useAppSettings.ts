/*
 * The settings Rust keeps for the whole app (Git, online, trusted folders, the folders Haunt keeps its files in): read
 * each time Settings opens, and each change saved at once. One copy, for Settings' sections.
 */
import type { AppSettings } from '~/bindings'

const app = shallowRef<AppSettings | null>(null)
const problem = ref('')

async function load(): Promise<void> {
  problem.value = ''
  try {
    app.value = await haunt.appSettings()
  } catch (error) {
    problem.value = errorMessage(error, t('composables.useAppSettings.notRead'))
  }
}

/** Keeps the settings a change returns; a change that fails says so with `failed` and keeps the last ones. */
async function save(change: Promise<AppSettings>, failed = t('common.notSaved')): Promise<boolean> {
  try {
    app.value = await change
    return true
  } catch (error) {
    useToast().add({ title: failed, description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
    return false
  }
}

async function saveOnline(hostTools: boolean, updateCheck: boolean): Promise<void> {
  if (await save(haunt.setOnline(hostTools, updateCheck))) useHostPulls().enabled.value = hostTools
}

export function useAppSettings() {
  return { app, problem, load, save, saveOnline }
}
