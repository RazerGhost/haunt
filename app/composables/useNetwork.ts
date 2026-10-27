import type { LastFetch, PullMode } from '~/bindings'

/*
 * Fetch, pull and push, from the toolbar and the tags' menu: each runs in Rust (git/write/network.rs), which sends
 * git's progress as it comes (shown in the toolbar, with a way to stop) and git's questions (a password, a key's
 * passphrase, whether to trust a host) as news: they are asked here, in the app's dialog, and the answer goes straight
 * back to git. What git decides for you is asked too: merge or rebase when the branches went apart, which remote the
 * first time, and whether to overwrite when the remote has commits you do not. One copy for the app;
 * plugins/haunt.client.ts starts it.
 */

type Op = 'fetch' | 'pull' | 'push'

/** The fetch, pull or push going on in the shown repository, with git's latest progress line. */
const running = shallowRef<{ op: Op, text: string } | null>(null)
/** When the shown repository was last fetched, and how it went (Rust keeps it while Haunt runs). */
const lastFetch = shallowRef<LastFetch | null>(null)
/** The question of git's shown now, so it can be closed when git stops waiting. */
let questionShown: number | null = null

/** The run that finished while the window was hidden, said when it is shown again. */
let finishedAway: Op | null = null

export function startNetwork(): void {
  const { repo, onNews, gitChanged } = repositories
  const dialog = useDialog()
  const toast = useToast()
  const readLast = async () => {
    const at = repo.value
    const found = at ? await haunt.lastFetch(at).catch(() => null) : null
    // Another tab by now: its own read is on its way.
    if (repo.value === at) lastFetch.value = found
  }
  watch(repo, () => { running.value = null })
  document.addEventListener('visibilitychange', () => {
    if (document.hidden || !finishedAway) return
    const op = finishedAway
    finishedAway = null
    const last = lastFetch.value
    toast.add({ title: t(`composables.useNetwork.finishedAway.${op}`), description: last && !last.ok ? t('composables.useNetwork.lastFetchFailed') : undefined, icon: 'i-lucide-bell' })
  })
  // A background fetch says nothing but that the refs changed: read when it last ran then, too.
  watch([repo, gitChanged('refs')], () => void readLast(), { immediate: true })
  onNews((news) => {
    if (news.type === 'network') {
      running.value = news.done ? null : { op: news.op, text: news.text ?? running.value?.text ?? '' }
      // git is done (or was stopped): a question still open has no one waiting for it.
      if (news.done) void readLast()
      if (news.done && document.hidden) finishedAway = news.op
      // The system says so too, when Haunt's window is not in front (Rust checks).
      if (news.done) void haunt.notify(t(`composables.useNetwork.finished.${news.op}`), folderName(repo.value ?? '')).catch(() => {})
      if (news.done && questionShown !== null) {
        const id = questionShown
        questionShown = null
        dialog.asking.value?.done(null)
        void haunt.answerQuestion(id, null).catch(() => {})
      }
    } else if (news.type === 'ask') {
      void answer(news.id, news.prompt, news.secret, news.yesNo)
    } else if (news.type === 'fetched') {
      const title = t('composables.useNetwork.newCommits', { count: news.commits })
      // The system says so too, when Haunt's window is not in front (Rust checks).
      void haunt.notify(title, folderName(news.repo ?? repo.value ?? '')).catch(() => {})
      toast.add({
        title,
        description: t('composables.useNetwork.fetchedBackground'),
        icon: 'i-lucide-cloud-download',
        actions: [{ label: t('composables.useNetwork.seeThem'), color: 'neutral', variant: 'outline', onClick: () => { void navigateTo('/incoming') } }]
      })
    } else if (news.type === 'notice') {
      toast.add({ title: news.message, color: 'warning', icon: 'i-lucide-folder-x' })
    } else if (news.type === 'asked' && questionShown === news.id) {
      questionShown = null
      dialog.asking.value?.done(null)
    }
  })

  async function answer(id: number, prompt: string, secret: boolean, yesNo: boolean): Promise<void> {
    questionShown = id
    let text: string | null
    if (yesNo) {
      const chosen = await dialog.choose({
        title: t('composables.useNetwork.gitAsks'),
        description: prompt,
        choices: [{ label: t('composables.useNetwork.yes'), value: 'yes', color: 'primary' }, { label: t('composables.useNetwork.no'), value: 'no' }]
      })
      text = chosen
    } else {
      const typed = await dialog.prompt({
        title: t('composables.useNetwork.gitAsks'),
        description: t('composables.useNetwork.promptHelp', { prompt: prompt.replace(/:\s*$/, '') }),
        input: { label: secret ? t('composables.useNetwork.password') : t('composables.useNetwork.answer'), value: '', secret, required: false },
        confirm: t('composables.useNetwork.send')
      })
      text = typed ? typed.text : null
    }
    if (questionShown !== id) return
    questionShown = null
    try {
      await haunt.answerQuestion(id, text)
    } catch { /* git stopped waiting */ }
  }
}

export function useNetwork() {
  const { repo, info } = useRepositories()
  const { current } = useRefs()
  const dialog = useDialog()
  const toast = useToast()
  const trusted = computed(() => info.value?.trusted ?? false)

  const { failed } = useFailure()

  async function fetch(): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      await haunt.fetch(at)
    } catch (error) {
      failed(t('composables.useNetwork.fetchFailed'), error)
    }
  }

  async function pull(mode: PullMode = 'fastForward'): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      const pulled = await haunt.pull(at, mode)
      if (pulled === 'conflicts') {
        toast.add({ title: t('composables.useNetwork.conflicts'), description: t('composables.useNetwork.conflictsHelp'), color: 'warning', icon: 'i-lucide-git-merge' })
        void navigateTo('/conflicts')
      } else if (pulled === 'upToDate') {
        toast.add({ title: t('composables.useNetwork.upToDate'), description: t('composables.useNetwork.upToDateHelp', { upstream: current.value?.upstream ?? t('composables.useNetwork.theRemoteBranch') }), icon: 'i-lucide-check' })
      } else if (pulled === 'diverged') {
        // Settings may have decided already (Git: when the branches went apart).
        const setting = await haunt.appSettings().then(found => found.pull).catch(() => 'ask' as const)
        if (setting !== 'ask') return await pull(setting)
        const branch = current.value?.name ?? t('composables.useNetwork.yourBranch')
        const chosen = await dialog.choose<PullMode>({
          title: t('composables.useNetwork.mergeOrRebase'),
          description: t('composables.useNetwork.divergedHelp', { branch, upstream: current.value?.upstream ?? t('composables.useNetwork.itsRemoteBranch') }),
          choices: [{ label: t('composables.useNetwork.rebase'), value: 'rebase', color: 'primary' }, { label: t('composables.useNetwork.merge'), value: 'merge' }]
        })
        if (chosen) await pull(chosen)
      }
    } catch (error) {
      failed(t('composables.useNetwork.pullFailed'), error)
    }
  }

  /** Pushes the current branch; `remote` and `force` come from the questions it may ask. */
  async function push(remote: string | null = null, force = false): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      const pushed = await haunt.push(at, remote, force)
      if (pushed.kind === 'chooseRemote') {
        const chosen = await dialog.choose({
          title: t('composables.useNetwork.whichRemote'),
          description: t('composables.useNetwork.whichRemoteHelp', { name: current.value?.name ?? t('composables.useNetwork.thisBranch') }),
          choices: pushed.remotes.map((name, index) => ({ label: name, value: name, color: index === 0 ? 'primary' as const : undefined }))
        })
        if (chosen) await push(chosen)
      } else if (pushed.kind === 'rejected') {
        const upstream = current.value?.upstream ?? t('composables.useNetwork.theRemoteBranch')
        const chosen = await dialog.choose({
          title: t('composables.useNetwork.rejected'),
          description: t('composables.useNetwork.rejectedHelp', { upstream }),
          choices: [{ label: t('composables.useNetwork.pullFirst'), value: 'pull', color: 'primary' }, { label: t('composables.useNetwork.overwrite'), value: 'force', color: 'error' }]
        })
        if (chosen === 'pull') await pull()
        else if (chosen === 'force') await push(remote, true)
      }
    } catch (error) {
      failed(t('composables.useNetwork.pushFailed'), error)
    }
  }

  async function pushTag(name: string, remote: string | null = null): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      const pushed = await haunt.pushTag(at, name, remote)
      if (pushed.kind === 'chooseRemote') {
        const chosen = await dialog.choose({
          title: t('composables.useNetwork.tagWhichRemote', { name }),
          description: t('composables.useNetwork.severalRemotes'),
          choices: pushed.remotes.map((remote, index) => ({ label: remote, value: remote, color: index === 0 ? 'primary' as const : undefined }))
        })
        if (chosen) await pushTag(name, chosen)
      } else {
        toast.add({ title: t('composables.useNetwork.tagPushed', { name }), icon: 'i-lucide-check' })
      }
    } catch (error) {
      failed(t('composables.useNetwork.tagNotPushed', { name }), error)
    }
  }

  async function stop(): Promise<void> {
    if (repo.value) await haunt.stopNetwork(repo.value).catch(() => false)
  }

  return { running, trusted, lastFetch, fetch, pull, push, pushTag, stop }
}
