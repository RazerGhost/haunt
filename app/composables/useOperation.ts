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
      toast.add({ title: 'Stopped on conflicts', description: 'Resolve them, then continue (or abort to put everything back).', color: 'warning', icon: 'i-lucide-git-merge' })
      void router.push('/conflicts')
    } else if (outcome === 'upToDate') {
      toast.add({ title: upToDate, icon: 'i-lucide-check' })
    }
  }

  const branch = () => current.value?.name ?? 'the current branch'

  async function revert(sha: string, subject: string): Promise<void> {
    const ok = await dialog.confirm({ title: 'Revert this commit?', description: `A new commit on ${branch()} undoes "${subject}". The original stays in the history.`, confirm: 'Revert' })
    if (ok) after(await run('The commit was not reverted', at => haunt.revert(at, sha)), 'There was nothing to take back.')
  }

  async function cherryPick(sha: string, subject: string): Promise<void> {
    const ok = await dialog.confirm({ title: `Cherry-pick onto ${branch()}?`, description: `The changes of "${subject}" again, as a new commit on ${branch()}.`, confirm: 'Cherry-pick' })
    if (ok) after(await run('The commit was not cherry-picked', at => haunt.cherryPick(at, sha)), `${branch()} has these changes already.`)
  }

  /** The remote branches a commit of the current branch is on: changing history there means a force push later. */
  async function remotesOf(sha: string): Promise<string[]> {
    return repo.value ? await haunt.onRemotes(repo.value, sha).catch(() => []) : []
  }

  async function reset(sha: string, label: string): Promise<void> {
    const pushed = await remotesOf('HEAD')
    const warning = pushed.length ? ` Commits after it are on ${pushed.join(', ')}: pushing then needs Overwrite.` : ''
    const mode = await dialog.choose<ResetMode>({
      title: `Reset ${branch()} to ${label}?`,
      description: `${branch()} moves back to this commit. Soft keeps later changes staged, mixed keeps them unstaged, hard discards them; your local changes are saved to Recently discarded.${warning}`,
      choices: [{ label: 'Mixed', value: 'mixed', color: 'primary' }, { label: 'Soft', value: 'soft' }, { label: 'Hard', value: 'hard', color: 'error' }]
    })
    if (!mode) return
    if (mode === 'hard' && !await dialog.confirm({ title: 'Reset hard?', description: `The commits after ${label} leave ${branch()}, and your local changes are replaced (saved first).`, confirm: 'Reset hard', danger: true })) return
    const before = refs.value?.sha
    if (await run('The branch was not reset', at => haunt.reset(at, sha, mode)) === undefined || !before) return
    const at = repo.value
    // A hard reset saved your local changes first: Undo brings them back too.
    const saved = mode === 'hard' && at ? await haunt.discarded(at).then(list => list.find(entry => Date.now() / 1000 - entry.at < 60)).catch(() => undefined) : undefined
    toast.add({
      title: `${branch()} reset to ${label}`,
      icon: 'i-lucide-history',
      duration: 12000,
      actions: [{
        label: 'Undo',
        color: 'neutral',
        variant: 'outline',
        onClick: () => {
          void run('Not undone', async (where) => {
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
      title: 'Undo the last commit?',
      description: `"${subject}" leaves ${branch()}; its changes are staged again, ready to commit.${pushed.length ? ` It is on ${pushed.join(', ')} already: pushing after this needs Overwrite.` : ''}`,
      confirm: 'Undo commit',
      danger: pushed.length > 0
    })
    const before = refs.value?.sha
    if (!ok || await run('The commit was not undone', at => haunt.undoCommit(at)) === undefined || !before) return
    toast.add({
      title: 'Commit undone',
      description: `"${subject}": its changes are staged.`,
      icon: 'i-lucide-rotate-ccw',
      duration: 12000,
      // Back to the commit: a soft reset to it, with its changes still staged.
      actions: [{ label: 'Undo', color: 'neutral', variant: 'outline', onClick: () => { void run('Not undone', where => haunt.reset(where, before, 'soft')) } }]
    })
  }

  async function merge(name: string): Promise<void> {
    const ok = await dialog.confirm({ title: `Merge ${name} into ${branch()}?`, description: `${branch()} gets ${name}'s commits: a fast-forward if possible, otherwise a merge commit.`, confirm: 'Merge' })
    if (ok) after(await run(`${name} was not merged`, at => haunt.merge(at, name)), `${branch()} has everything of ${name} already.`)
  }

  async function proceed(): Promise<void> {
    const outcome = await run('Not continued', at => haunt.continueOperation(at))
    if (outcome === 'conflicts') toast.add({ title: 'More conflicts', description: 'The next commit conflicts too.', color: 'warning', icon: 'i-lucide-git-merge' })
    else if (outcome === 'done') void router.push('/commits')
  }

  async function abort(): Promise<void> {
    const going = operation.value
    if (!going) return
    const ok = await dialog.confirm({ title: `Abort: ${going.title}?`, description: 'The branch and files go back to before it started. Resolved conflicts are lost.', confirm: 'Abort', danger: true })
    if (ok && await run('Not aborted', at => haunt.abortOperation(at)) !== undefined) void router.push('/commits')
  }

  return { operation, busy, revert, cherryPick, reset, undoCommit, merge, proceed, abort }
}
