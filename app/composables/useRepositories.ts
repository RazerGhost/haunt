import type { DesktopLook, GitPart, RootInfo } from '~~/shared/types'
import type { ThemeFileState } from '~/bindings'
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
/** The desktop's light or dark, accent and window buttons (Linux; all null elsewhere), followed as they change. */
const desktop = shallowRef<DesktopLook>({ dark: null, accent: null, buttons: null })
/** theme.json (theme_file.rs): what it sets wins over the profile's own look. */
const themeFile = shallowRef<ThemeFileState>({ path: '', theme: null, problem: null, leftOut: [] })
let themeFileHeard = false
/** The window has the system's frame and title bar (Settings, Appearance), so Haunt draws no window buttons. */
const systemTitleBar = ref(false)
/** The window is see-through (Linux, theme.json's opacity at start): only then does that opacity apply. */
const transparent = ref(false)
/** The repository screen is open on request (it also shows when no repository is open). */
const picking = ref(false)
/**
 * A repository whose folder is missing, explained in place of the screen (MissingRepo): the shown one when its folder
 * went (missing.rs), or a missing tab or recent repository clicked.
 */
const gone = ref<string | null>(null)
watch(picking, (now) => { if (now) gone.value = null })
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
    if (news.type === 'root' || news.type === 'trust' || news.type === 'tabs') void loadInfo()
    // Its folder came back and it is shown again.
    if (news.type === 'root' && news.root !== null && news.root === gone.value) gone.value = null
    if (news.type === 'missing') {
      gone.value = news.root
      void loadInfo()
    }
    if (news.type === 'desktop') desktop.value = news.desktop
    if (news.type === 'themeFile') {
      themeFile.value = news.state as ThemeFileState
      themeFileHeard = true
    }
    for (const listener of listeners) listener(news)
  })
  try {
    const started = await haunt.startup()
    problems.value = started.problems
    tiling.value = started.tiling
    // The news may have come first.
    if (desktop.value.dark === null && desktop.value.accent === null && desktop.value.buttons === null) desktop.value = started.desktop
    if (!themeFileHeard) themeFile.value = started.themeFile
    systemTitleBar.value = started.systemTitleBar
    transparent.value = started.transparent
  } catch { /* said again on the first call */ }
  await loadInfo()
  // Back in the window: what git did meanwhile that the watchers missed (a drive's files, a network share) comes as
  // the usual news, only for the parts that changed.
  window.addEventListener('focus', () => {
    if (repo.value) haunt.recheck(repo.value).catch(() => { /* another tab by now */ })
  })
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
      gone.value = null
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

  /** Shows why a tab or recent repository cannot open: its folder is missing. */
  function showMissing(path: string): void {
    gone.value = path
    picking.value = false
  }

  /** The missing repository at `old` is at `path` now: moved there, with what the window keeps of it, and shown. */
  async function relocate(old: string, path: string): Promise<string | null> {
    try {
      rememberRoute()
      const route = tabRoutes.value[old]
      info.value = await haunt.relocate(old, path)
      const now = info.value.root
      if (now) movePersisted(old, now)
      gone.value = null
      picking.value = false
      void router.push(route ?? '/commits')
      return null
    } catch (error) {
      return errorMessage(error, t('composables.useRepositories.notLocated'))
    }
  }

  /** Takes a missing repository away: its tab, its recent entry, and what the window keeps of it. */
  async function forgetMissing(path: string): Promise<void> {
    try {
      if (info.value?.all_tabs.includes(path)) info.value = await haunt.close(path)
      if (info.value?.all_recent.includes(path)) info.value = await haunt.forgetRecent(path)
      movePersisted(path, null)
      gone.value = null
      if (!info.value?.root) picking.value = true
    } catch (error) {
      toast.add({ title: t('composables.useRepositories.notClosed'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }

  /** Opens the tab closed last again; false when there is none. */
  async function reopenClosed(): Promise<boolean> {
    const path = closed.value.at(-1)
    if (!path) return false
    closed.value = closed.value.slice(0, -1)
    return (await chooseRoot(path)) === null
  }

  return { info, repo, problems, tiling, desktop, themeFile, systemTitleBar, picking, gone, closed, gitChanged, chooseRoot, closeTab, reopenClosed, setTrust, rememberRoute, onNews, showMissing, relocate, forgetMissing }
}

/**
 * The state without the router, for what starts before the window (the plugin, the composables' start functions).
 * Last in the file: Nuxt's export scanner (mlly) misses an exported function that follows a one-line exported object.
 */
export const repositories = { info, repo, problems, desktop, themeFile, transparent, picking, gitChanged, onNews }
