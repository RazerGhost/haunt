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
    const what = { fetch: 'Fetching', pull: 'Pulling', push: 'Pushing' }[finishedAway]
    finishedAway = null
    const last = lastFetch.value
    toast.add({ title: `${what} finished while Haunt was in the background`, description: last && !last.ok ? 'The last fetch failed: see Fetch\'s tooltip.' : undefined, icon: 'i-lucide-bell' })
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
      if (news.done) void haunt.notify(`${{ fetch: 'Fetching', pull: 'Pulling', push: 'Pushing' }[news.op]} finished`, folderName(repo.value ?? '')).catch(() => {})
      if (news.done && questionShown !== null) {
        const id = questionShown
        questionShown = null
        dialog.asking.value?.done(null)
        void haunt.answerQuestion(id, null).catch(() => {})
      }
    } else if (news.type === 'ask') {
      void answer(news.id, news.prompt, news.secret, news.yesNo)
    } else if (news.type === 'fetched') {
      const title = `${news.commits} new ${news.commits === 1 ? 'commit' : 'commits'} on the remote branch`
      // The system says so too, when Haunt's window is not in front (Rust checks).
      void haunt.notify(title, folderName(news.repo ?? repo.value ?? '')).catch(() => {})
      toast.add({
        title,
        description: 'Fetched in the background.',
        icon: 'i-lucide-cloud-download',
        actions: [{ label: 'See them', color: 'neutral', variant: 'outline', onClick: () => { void navigateTo('/incoming') } }]
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
        title: 'git asks',
        description: prompt,
        choices: [{ label: 'Yes', value: 'yes', color: 'primary' }, { label: 'No', value: 'no' }]
      })
      text = chosen
    } else {
      const typed = await dialog.prompt({
        title: 'git asks',
        description: `${prompt.replace(/:\s*$/, '')}. Haunt passes your answer to git and keeps nothing.`,
        input: { label: secret ? 'Password or passphrase' : 'Answer', value: '', secret, required: false },
        confirm: 'Send'
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
      failed('Fetching did not finish', error)
    }
  }

  async function pull(mode: PullMode = 'fastForward'): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      const pulled = await haunt.pull(at, mode)
      if (pulled === 'conflicts') {
        toast.add({ title: 'Pulling stopped on conflicts', description: 'Resolve them, then continue (or abort to put everything back).', color: 'warning', icon: 'i-lucide-git-merge' })
        void navigateTo('/conflicts')
      } else if (pulled === 'upToDate') {
        toast.add({ title: 'Already up to date', description: `${current.value?.upstream ?? 'The remote branch'} has nothing new.`, icon: 'i-lucide-check' })
      } else if (pulled === 'diverged') {
        // Settings may have decided already (Git: when the branches went apart).
        const setting = await haunt.appSettings().then(found => found.pull).catch(() => 'ask' as const)
        if (setting !== 'ask') return await pull(setting)
        const branch = current.value?.name ?? 'Your branch'
        const chosen = await dialog.choose<PullMode>({
          title: 'Merge or rebase?',
          description: `Both ${branch} and ${current.value?.upstream ?? 'its remote branch'} have new commits. Rebase puts yours on top; merge joins them.`,
          choices: [{ label: 'Rebase', value: 'rebase', color: 'primary' }, { label: 'Merge', value: 'merge' }]
        })
        if (chosen) await pull(chosen)
      }
    } catch (error) {
      failed('Pulling did not finish', error)
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
          title: 'Push to which remote?',
          description: `${current.value?.name ?? 'This branch'} has no remote branch yet; Haunt creates one with the same name.`,
          choices: pushed.remotes.map((name, index) => ({ label: name, value: name, color: index === 0 ? 'primary' as const : undefined }))
        })
        if (chosen) await push(chosen)
      } else if (pushed.kind === 'rejected') {
        const upstream = current.value?.upstream ?? 'The remote branch'
        const chosen = await dialog.choose({
          title: 'The remote has commits you do not',
          description: `${upstream} has commits you do not. Pull first, or overwrite (only if it has not moved since your last fetch).`,
          choices: [{ label: 'Pull first', value: 'pull', color: 'primary' }, { label: 'Overwrite', value: 'force', color: 'error' }]
        })
        if (chosen === 'pull') await pull()
        else if (chosen === 'force') await push(remote, true)
      }
    } catch (error) {
      failed('Pushing did not finish', error)
    }
  }

  async function pushTag(name: string, remote: string | null = null): Promise<void> {
    const at = repo.value
    if (!at || running.value) return
    try {
      const pushed = await haunt.pushTag(at, name, remote)
      if (pushed.kind === 'chooseRemote') {
        const chosen = await dialog.choose({
          title: `Push ${name} to which remote?`,
          description: 'This repository has several remotes.',
          choices: pushed.remotes.map((remote, index) => ({ label: remote, value: remote, color: index === 0 ? 'primary' as const : undefined }))
        })
        if (chosen) await pushTag(name, chosen)
      } else {
        toast.add({ title: `Pushed ${name}`, icon: 'i-lucide-check' })
      }
    } catch (error) {
      failed(`${name} was not pushed`, error)
    }
  }

  async function stop(): Promise<void> {
    if (repo.value) await haunt.stopNetwork(repo.value).catch(() => false)
  }

  return { running, trusted, lastFetch, fetch, pull, push, pushTag, stop }
}
