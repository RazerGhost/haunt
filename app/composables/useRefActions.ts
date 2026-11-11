import type { DroppedStash } from '~/bindings'

/*
 * What can be done with branches, stashes and tags, from the sidebar, the toolbar and the commit list: each asks what it
 * needs (a name, what to do with local changes, a yes before losing something), runs the write in Rust
 * (git/write/refs.rs) and says what went wrong, in git's words. The sidebar's refs are read again on the git news Rust
 * sends after every write.
 */

const dropped = shallowRef<DroppedStash[]>([])

export function useRefActions() {
  const { repo } = useRepositories()
  const { localChanges, refs } = useRefs()
  const dialog = useDialog()
  const toast = useToast()
  const failure = useFailure()
  const busy = ref(false)

  async function run(what: string, write: (at: string) => Promise<unknown>): Promise<boolean> {
    const at = repo.value
    if (!at) return false
    busy.value = true
    try {
      await write(at)
      return true
    } catch (error) {
      failure.failed(what, error)
      return false
    } finally {
      busy.value = false
      void loadDropped()
    }
  }

  async function loadDropped(): Promise<void> {
    if (!repo.value) return
    try {
      dropped.value = await haunt.droppedStashes(repo.value)
    } catch {
      dropped.value = []
    }
  }

  /** A new branch at `at` (a branch, tag or commit; HEAD when null), switched to when asked. */
  async function newBranch(at: { rev: string, label: string } | null = null): Promise<void> {
    const answer = await dialog.prompt({
      title: t('composables.useRefActions.newBranch'),
      description: at ? t('composables.useRefActions.from', { name: at.label }) : t('composables.useRefActions.fromHere'),
      input: { label: t('composables.useRefActions.name'), value: '', placeholder: 'feature/name' },
      checkbox: { label: t('composables.useRefActions.switchToIt'), value: true },
      confirm: t('composables.useRefActions.createBranch')
    })
    if (answer) await run(t('composables.useRefActions.branchNotCreated'), where => haunt.createBranch(where, answer.text, at?.rev ?? null, answer.checked))
  }

  /** Switches to a branch (a remote one through a local branch that tracks it), asking about local changes first. */
  async function switchTo(name: string): Promise<void> {
    let mode: 'carry' | 'stash' = 'carry'
    if (localChanges.value) {
      const chosen = await dialog.choose({
        title: t('composables.useRefActions.switchTitle', { name }),
        description: t('composables.useRefActions.switchChanges', { count: localChanges.value }),
        choices: [
          { label: t('composables.useRefActions.carry'), value: 'carry', color: 'primary' },
          { label: t('composables.useRefActions.stashFirst'), value: 'stash' }
        ]
      })
      if (!chosen) return
      mode = chosen
    }
    await run(t('composables.useRefActions.notSwitched', { name }), where => haunt.switchBranch(where, name, mode))
  }

  /** A branch that went apart from its remote branch: keeps a copy of it (`main-before-2026-11-11`), then moves it to
   * the remote branch, after asking. Nothing is lost: the copy has every commit only it had. */
  async function matchUpstream(branch: { name: string, upstream: string | null, ahead: number }): Promise<void> {
    if (!branch.upstream) return
    const day = new Date().toISOString().slice(0, 10)
    const taken = new Set(refs.value?.branches.map(one => one.name))
    let backup = `${branch.name}-before-${day}`
    for (let n = 2; taken.has(backup); n++) backup = `${branch.name}-before-${day}-${n}`
    const ok = await dialog.confirm({
      title: t('composables.useRefActions.matchTitle', { name: branch.name, upstream: branch.upstream }),
      description: t('composables.useRefActions.matchHelp', { name: branch.name, upstream: branch.upstream, backup, count: branch.ahead }),
      confirm: t('composables.useRefActions.match')
    })
    if (!ok) return
    if (await run(t('composables.useRefActions.notMatched', { name: branch.name }), where => haunt.matchUpstream(where, branch.name, backup))) {
      toast.add({ title: t('composables.useRefActions.matched', { name: branch.name, upstream: branch.upstream }), description: t('composables.useRefActions.copyKept', { backup }), icon: 'i-lucide-check', color: 'success' })
    }
  }

  async function renameBranch(name: string): Promise<void> {
    const answer = await dialog.prompt({ title: t('composables.useRefActions.renameTitle', { name }), input: { label: t('composables.useRefActions.newName'), value: name }, confirm: t('composables.useRefActions.rename') })
    if (answer && answer.text !== name) await run(t('composables.useRefActions.branchNotRenamed'), where => haunt.renameBranch(where, name, answer.text))
  }

  /** Deletes a branch; one with commits no other branch has only after saying how many would be lost. */
  async function deleteBranch(name: string): Promise<void> {
    const at = repo.value
    if (!at) return
    let lost = 0
    try {
      lost = await haunt.unmerged(at, name)
    } catch { /* asked below as if merged; Rust checks again */ }
    const ok = await dialog.confirm({
      title: t('composables.useRefActions.deleteTitle', { name }),
      description: lost
        ? t('composables.useRefActions.lost', { count: lost })
        : t('composables.useRefActions.nothingLost'),
      confirm: t('composables.useRefActions.deleteBranch'),
      danger: lost > 0
    })
    if (ok) await run(t('composables.useRefActions.branchNotDeleted'), where => haunt.deleteBranch(where, name, lost > 0))
  }

  /** Deletes several branches after one question, which says how many have commits on no other branch. */
  async function deleteBranches(names: string[]): Promise<void> {
    const at = repo.value
    if (!at || !names.length) return
    if (names.length === 1) return deleteBranch(names[0]!)
    const lost = await Promise.all(names.map(name => haunt.unmerged(at, name).catch(() => 0)))
    const losing = lost.filter(count => count > 0).length
    const ok = await dialog.confirm({
      title: t('composables.useRefActions.deleteSeveralTitle', { count: names.length }),
      description: losing ? t('composables.useRefActions.severalLost', { count: losing }) : t('composables.useRefActions.nothingLostSeveral'),
      confirm: t('composables.useRefActions.deleteSeveral', { count: names.length }),
      danger: losing > 0
    })
    if (ok) {
      await run(t('composables.useRefActions.branchNotDeleted'), async (where) => {
        for (const [index, name] of names.entries()) await haunt.deleteBranch(where, name, lost[index]! > 0)
      })
    }
  }

  async function stash(): Promise<void> {
    const answer = await dialog.prompt({
      title: t('composables.useRefActions.stashTitle'),
      input: { label: t('composables.useRefActions.message'), value: '', placeholder: t('composables.useRefActions.messagePlaceholder'), required: false },
      checkbox: { label: t('composables.useRefActions.untracked'), value: true },
      confirm: t('composables.useRefActions.stash')
    })
    if (answer) await run(t('composables.useRefActions.notStashed'), where => haunt.stash(where, answer.text || null, answer.checked, []))
  }

  const applyStash = (index: number, pop: boolean) => run(pop ? t('composables.useRefActions.notPopped') : t('composables.useRefActions.notApplied'), where => haunt.applyStash(where, index, pop))

  async function dropStash(index: number, message: string): Promise<void> {
    const ok = await dialog.confirm({ title: t('composables.useRefActions.dropTitle'), description: t('composables.useRefActions.dropHelp', { message }), confirm: t('composables.useRefActions.drop'), danger: true })
    if (!ok || !await run(t('composables.useRefActions.notDropped'), where => haunt.dropStash(where, index))) return
    const at = repo.value
    const kept = at ? await haunt.droppedStashes(at).then(list => list[0]).catch(() => undefined) : undefined
    if (kept) {
      toast.add({
        title: t('composables.useRefActions.dropped'),
        description: t('composables.useRefActions.droppedHelp', { message }),
        icon: 'i-lucide-archive-x',
        duration: 12000,
        actions: [{ label: t('common.undo'), color: 'neutral', variant: 'outline', onClick: () => { void restoreStash(kept.id) } }]
      })
    }
  }

  /** Drops several stashes after one question; the highest index first, so the others keep theirs until dropped. */
  async function dropStashes(stashes: { index: number, message: string }[]): Promise<void> {
    if (stashes.length === 1) return dropStash(stashes[0]!.index, stashes[0]!.message)
    const count = stashes.length
    const ok = await dialog.confirm({ title: t('composables.useRefActions.dropSeveralTitle', { count }), description: t('composables.useRefActions.dropSeveralHelp', { count }), confirm: t('composables.useRefActions.drop'), danger: true })
    const indices = stashes.map(stash => stash.index).sort((a, b) => b - a)
    if (!ok || !await run(t('composables.useRefActions.notDropped'), async (where) => {
      for (const index of indices) await haunt.dropStash(where, index)
    })) return
    const at = repo.value
    const kept = at ? await haunt.droppedStashes(at).then(list => list.slice(0, count)).catch(() => []) : []
    toast.add({
      title: t('composables.useRefActions.droppedSeveral', { count }),
      description: t('composables.useRefActions.droppedSeveralHelp'),
      icon: 'i-lucide-archive-x',
      duration: 12000,
      actions: kept.length === count ? [{ label: t('common.undo'), color: 'neutral', variant: 'outline', onClick: () => { void restoreStashes(kept.map(entry => entry.id)) } }] : []
    })
  }

  const restoreStash = (id: string) => run(t('composables.useRefActions.notRestored'), where => haunt.restoreStash(where, id))
  /** Brings several dropped stashes back, the one dropped first first, so they come back in their order. */
  const restoreStashes = (ids: string[]) => run(t('composables.useRefActions.notRestored'), async (where) => {
    for (const id of [...ids].reverse()) await haunt.restoreStash(where, id)
  })

  async function newTag(at: { rev: string, label: string } | null = null): Promise<void> {
    const answer = await dialog.prompt({
      title: t('composables.useRefActions.newTag'),
      description: at ? t('composables.useRefActions.on', { name: at.label }) : t('composables.useRefActions.onHere'),
      input: { label: t('composables.useRefActions.name'), value: '', placeholder: 'v1.2.0' },
      note: { label: t('composables.useRefActions.tagMessage'), value: '', placeholder: t('composables.useRefActions.tagMessagePlaceholder') },
      confirm: t('composables.useRefActions.createTag')
    })
    if (answer) await run(t('composables.useRefActions.tagNotCreated'), where => haunt.createTag(where, answer.text, at?.rev ?? null, answer.note || null))
  }

  async function deleteTag(name: string): Promise<void> {
    const ok = await dialog.confirm({ title: t('composables.useRefActions.deleteTagTitle', { name }), description: t('composables.useRefActions.deleteTagHelp'), confirm: t('composables.useRefActions.deleteTag'), danger: true })
    if (ok) await run(t('composables.useRefActions.tagNotDeleted'), where => haunt.deleteTag(where, name))
  }

  /** Deletes several tags after one question. */
  async function deleteTags(names: string[]): Promise<void> {
    if (names.length === 1) return deleteTag(names[0]!)
    const ok = await dialog.confirm({ title: t('composables.useRefActions.deleteTagsTitle', { count: names.length }), description: t('composables.useRefActions.deleteTagHelp'), confirm: t('composables.useRefActions.deleteSeveral', { count: names.length }), danger: true })
    if (ok) {
      await run(t('composables.useRefActions.tagNotDeleted'), async (where) => {
        for (const name of names) await haunt.deleteTag(where, name)
      })
    }
  }

  return { busy, dropped, loadDropped, newBranch, switchTo, matchUpstream, renameBranch, deleteBranch, deleteBranches, deleteTags, stash, applyStash, dropStash, dropStashes, restoreStash, restoreStashes, newTag, deleteTag }
}
