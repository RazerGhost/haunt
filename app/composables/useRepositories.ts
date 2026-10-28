import type { GitPart, RootInfo } from '~~/shared/types'
import type { News } from '~/utils/bridge'

/*
 * The repositories: the open tabs, the one shown (`repo`, the name every call about it uses), its trust, what this
 * machine lacks (a JavaScript runtime, git), and the news Rust sends about the shown repository. Switching tabs
 * remembers where each tab was and goes back there. One copy for the app; plugins/haunt.client.ts starts it.
 */

const info = shallowRef<RootInfo | null>(null)
/** What is missing on this machine, said at start (then nothing can be scanned). */
const problems = ref<string[]>([])
/** The tiling compositor the window is in (Hyprland, Sway…): it places and sizes the window itself. Null elsewhere. */
const tiling = ref<string | null>(null)
/** The repository screen is open on request (it also shows when no repository is open). */
const picking = ref(false)
/** Counts the 'git' news per part of git (HEAD, refs, the index, the stash), all in this one place. */
const git = reactive<Record<GitPart, number>>({ head: 0, refs: 0, index: 0, stash: 0 })
/** Changes when any of `parts` changed: what a view watches to read git again, for only the parts it shows. */
function gitChanged(...parts: GitPart[]) {
  return computed(() => parts.reduce((sum, part) => sum + git[part], 0))
}
const repo = computed(() => info.value?.root ?? null)
const listeners = new Set<(news: News) => void>()
/** Tabs closed while Haunt runs, the latest last, for reopening one. */
const closed = ref<string[]>([])

/** Where each tab was (its screen and what was selected), so going back to a tab goes back there. */
const tabRoutes = usePersisted<Record<string, string>>('tab-routes', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))

async function loadInfo(): Promise<void> {
  try {
    info.value = await haunt.info()
  } catch { /* Rust is starting: the next news reads it again */ }
}

/** Starts listening to Rust and reads what is open; called once, by the plugin. */
export async function startRepositories(): Promise<void> {
  await listen((news) => {
    // News for a repository that is no longer shown (an older tab's scanner, still finishing) is dropped here.
    if (news.repo !== null && news.repo !== repo.value && news.type !== 'root') return
    if (news.type === 'git') for (const part of news.parts) git[part]++
    if (news.type === 'root' || news.type === 'trust') void loadInfo()
    for (const listener of listeners) listener(news)
  })
  try {
    const started = await haunt.startup()
    problems.value = started.problems
    tiling.value = started.tiling
  } catch { /* said again on the first call */ }
  await loadInfo()
}

/** Runs `listener` on every piece of news about the shown repository; returns a function that stops it. */
function onNews(listener: (news: News) => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useRepositories() {
  const route = useRoute()
  const router = useRouter()
  const toast = useToast()

  /** Remembers where the shown tab is, so going back to it goes back there. */
  function rememberRoute(): void {
    if (!repo.value || picking.value) return
    tabRoutes.value = { ...tabRoutes.value, [repo.value]: route.fullPath }
  }

  /** Shows a repository, in its tab (a new one if it has none); resolves to an error message, or null. */
  async function chooseRoot(path: string): Promise<string | null> {
    try {
      rememberRoute()
      info.value = await haunt.open(path)
      picking.value = false
      void router.push(tabRoutes.value[info.value.root ?? ''] ?? '/commits')
      return null
    } catch (error) {
      return errorMessage(error, t('composables.useRepositories.notOpened'))
    }
  }

  /** Closes a repository's tab; closing the one shown shows the tab next to it. */
  async function closeTab(path: string): Promise<void> {
    const shown = repo.value === path
    try {
      info.value = await haunt.close(path)
      closed.value = [...closed.value.filter(other => other !== path), path].slice(-10)
      const { [path]: _gone, ...rest } = tabRoutes.value
      tabRoutes.value = rest
      if (shown) void router.push(tabRoutes.value[info.value.root ?? ''] ?? '/commits')
    } catch (error) {
      toast.add({ title: t('composables.useRepositories.notClosed'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }

  /** Lets the shown repository's own code run, or stops it; resolves to an error message, or null. */
  async function setTrust(trusted: boolean): Promise<string | null> {
    if (!repo.value) return null
    try {
      info.value = await haunt.trust(repo.value, trusted)
      return null
    } catch (error) {
      return errorMessage(error, t('composables.useRepositories.trustNotChanged'))
    }
  }

  /** Opens the tab closed last again; false when there is none. */
  async function reopenClosed(): Promise<boolean> {
    const path = closed.value.at(-1)
    if (!path) return false
    closed.value = closed.value.slice(0, -1)
    return (await chooseRoot(path)) === null
  }

  return { info, repo, problems, tiling, picking, closed, gitChanged, chooseRoot, closeTab, reopenClosed, setTrust, rememberRoute, onNews }
}

/**
 * The state without the router, for what starts before the window (the plugin, the composables' start functions).
 * Last in the file: Nuxt's export scanner (mlly) misses an exported function that follows a one-line exported object.
 */
export const repositories = { info, repo, problems, picking, gitChanged, onNews }
