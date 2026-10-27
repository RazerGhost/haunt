import type { Operation, Outcome, ResetMode } from '~/bindings'

/*
 * Undoing and merging (git/write/undo.rs, conflicts.rs), and the merge, revert, cherry-pick, pull or rebase that
 * stopped on conflicts: what is going on is read when a repository is shown and on every git news, and shown as a
 * banner under the toolbar; the conflicts are resolved on the Conflicts screen. Each action asks what it needs first
 * (how far a reset goes, a yes before history that is on a remote changes) and says what went wrong in git's words.
 * One copy for the app; plugins/haunt.client.ts starts it.
 */

const operation = shallowRef<Operation | null>(null)

export function startOperation(): void {
  const { repo, gitChanged } = repositories
  const changed = gitChanged('head', 'index', 'refs')
  const ask = latest(repo)
  async function load(): Promise<void> {
    try {
      const found = await ask.run(at => haunt.operation(at))
      if (found !== undefined) operation.value = found
    } catch { /* the next news reads it again */ }
  }
  watch(repo, (now) => {
    operation.value = null
    if (now) void load()
  }, { immediate: true })
  watch(changed, () => repo.value && void load())
  window.addEventListener('focus', () => repo.value && void load())
}

export function useOperation() {
  const { repo } = useRepositories()
  const { current, refs } = useRefs()
  const dialog = useDialog()
  const toast = useToast()
  const router = useRouter()
  const failure = useFailure()
  const busy = ref(false)

  async function run<T>(what: string, write: (at: string) => Promise<T>): Promise<T | undefined> {
    const at = repo.value
    if (!at || busy.value) return undefined
    busy.value = true
    try {
      return await write(at)
    } catch (error) {
      failure.failed(what, error)
      return undefined
    } finally {
      busy.value = false
    }
  }

  /** After a write that may stop on conflicts: to the Conflicts screen, or a word that there was nothing to do. */
  function after(outcome: Outcome | undefined, upToDate: string): void {
    if (outcome === 'conflicts') {
      toast.add({ title: t('composables.useOperation.stopped'), description: t('composables.useOperation.stoppedHelp'), color: 'warning', icon: 'i-lucide-git-merge' })
      void router.push('/conflicts')
    } else if (outcome === 'upToDate') {
      toast.add({ title: upToDate, icon: 'i-lucide-check' })
    }
  }

  const branch = () => current.value?.name ?? t('composables.useOperation.currentBranch')

  async function revert(sha: string, subject: string): Promise<void> {
    const ok = await dialog.confirm({ title: t('composables.useOperation.revertTitle'), description: t('composables.useOperation.revertDescription', { branch: branch(), subject }), confirm: t('composables.useOperation.revertConfirm') })
    if (ok) after(await run(t('composables.useOperation.notReverted'), at => haunt.revert(at, sha)), t('composables.useOperation.nothingToRevert'))
  }

  async function cherryPick(sha: string, subject: string): Promise<void> {
    const ok = await dialog.confirm({ title: t('composables.useOperation.cherryPickTitle', { branch: branch() }), description: t('composables.useOperation.cherryPickDescription', { branch: branch(), subject }), confirm: t('composables.useOperation.cherryPickConfirm') })
    if (ok) after(await run(t('composables.useOperation.notCherryPicked'), at => haunt.cherryPick(at, sha)), t('composables.useOperation.alreadyPicked', { branch: branch() }))
  }

  /** The remote branches a commit of the current branch is on: changing history there means a force push later. */
  async function remotesOf(sha: string): Promise<string[]> {
    return repo.value ? await haunt.onRemotes(repo.value, sha).catch(() => []) : []
  }

  async function reset(sha: string, label: string): Promise<void> {
    const pushed = await remotesOf('HEAD')
    const warning = pushed.length ? ` ${t('composables.useOperation.resetPushed', { remotes: pushed.join(', ') })}` : ''
    const mode = await dialog.choose<ResetMode>({
      title: t('composables.useOperation.resetTitle', { branch: branch(), label }),
      description: `${t('composables.useOperation.resetDescription', { branch: branch() })}${warning}`,
      choices: [{ label: t('composables.useOperation.mixed'), value: 'mixed', color: 'primary' }, { label: t('composables.useOperation.soft'), value: 'soft' }, { label: t('composables.useOperation.hard'), value: 'hard', color: 'error' }]
    })
    if (!mode) return
    if (mode === 'hard' && !await dialog.confirm({ title: t('composables.useOperation.resetHardTitle'), description: t('composables.useOperation.resetHardDescription', { branch: branch(), label }), confirm: t('composables.useOperation.resetHardConfirm'), danger: true })) return
    const before = refs.value?.sha
    if (await run(t('composables.useOperation.notReset'), at => haunt.reset(at, sha, mode)) === undefined || !before) return
    const at = repo.value
    // A hard reset saved your local changes first: Undo brings them back too.
    const saved = mode === 'hard' && at ? await haunt.discarded(at).then(list => list.find(entry => Date.now() / 1000 - entry.at < 60)).catch(() => undefined) : undefined
    toast.add({
      title: t('composables.useOperation.resetDone', { branch: branch(), label }),
      icon: 'i-lucide-history',
      duration: 12000,
      actions: [{
        label: t('common.undo'),
        color: 'neutral',
        variant: 'outline',
        onClick: () => {
          void run(t('composables.useOperation.notUndone'), async (where) => {
            await haunt.reset(where, before, mode)
            if (saved) await haunt.restore(where, saved.id)
          })
        }
      }]
    })
  }

  async function undoCommit(subject: string): Promise<void> {
    const pushed = await remotesOf('HEAD')
    const ok = await dialog.confirm({
      title: t('composables.useOperation.undoCommitTitle'),
      description: `${t('composables.useOperation.undoCommitDescription', { subject, branch: branch() })}${pushed.length ? ` ${t('composables.useOperation.undoCommitPushed', { remotes: pushed.join(', ') })}` : ''}`,
      confirm: t('composables.useOperation.undoCommitConfirm'),
      danger: pushed.length > 0
    })
    const before = refs.value?.sha
    if (!ok || await run(t('composables.useOperation.commitNotUndone'), at => haunt.undoCommit(at)) === undefined || !before) return
    toast.add({
      title: t('composables.useOperation.commitUndone'),
      description: t('composables.useOperation.commitUndoneDescription', { subject }),
      icon: 'i-lucide-rotate-ccw',
      duration: 12000,
      // Back to the commit: a soft reset to it, with its changes still staged.
      actions: [{ label: t('common.undo'), color: 'neutral', variant: 'outline', onClick: () => { void run(t('composables.useOperation.notUndone'), where => haunt.reset(where, before, 'soft')) } }]
    })
  }

  async function merge(name: string): Promise<void> {
    const ok = await dialog.confirm({ title: t('composables.useOperation.mergeTitle', { name, branch: branch() }), description: t('composables.useOperation.mergeDescription', { name, branch: branch() }), confirm: t('composables.useOperation.mergeConfirm') })
    if (ok) after(await run(t('composables.useOperation.notMerged', { name }), at => haunt.merge(at, name)), t('composables.useOperation.mergeUpToDate', { name, branch: branch() }))
  }

  async function proceed(): Promise<void> {
    const outcome = await run(t('composables.useOperation.notContinued'), at => haunt.continueOperation(at))
    if (outcome === 'conflicts') toast.add({ title: t('composables.useOperation.moreConflicts'), description: t('composables.useOperation.moreConflictsHelp'), color: 'warning', icon: 'i-lucide-git-merge' })
    else if (outcome === 'done') void router.push('/commits')
  }

  async function abort(): Promise<void> {
    const going = operation.value
    if (!going) return
    const ok = await dialog.confirm({ title: t('composables.useOperation.abortTitle', { title: going.title }), description: t('composables.useOperation.abortDescription'), confirm: t('composables.useOperation.abortConfirm'), danger: true })
    if (ok && await run(t('composables.useOperation.notAborted'), at => haunt.abortOperation(at)) !== undefined) void router.push('/commits')
  }

  return { operation, busy, revert, cherryPick, reset, undoCommit, merge, proceed, abort }
}
