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
  const { localChanges } = useRefs()
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
      title: 'New branch',
      description: at ? `From ${at.label}.` : 'From the commit you are on.',
      input: { label: 'Name', value: '', placeholder: 'feature/name' },
      checkbox: { label: 'Switch to it', value: true },
      confirm: 'Create branch'
    })
    if (answer) await run('The branch was not created', where => haunt.createBranch(where, answer.text, at?.rev ?? null, answer.checked))
  }

  /** Switches to a branch (a remote one through a local branch that tracks it), asking about local changes first. */
  async function switchTo(name: string): Promise<void> {
    let mode: 'carry' | 'stash' = 'carry'
    if (localChanges.value) {
      const chosen = await dialog.choose({
        title: `Switch to ${name}?`,
        description: `You have ${localChanges.value} ${localChanges.value === 1 ? 'file' : 'files'} with local changes.`,
        choices: [
          { label: 'Take them along', value: 'carry', color: 'primary' },
          { label: 'Stash them first', value: 'stash' }
        ]
      })
      if (!chosen) return
      mode = chosen
    }
    await run(`Haunt did not switch to ${name}`, where => haunt.switchBranch(where, name, mode))
  }

  async function renameBranch(name: string): Promise<void> {
    const answer = await dialog.prompt({ title: `Rename ${name}`, input: { label: 'New name', value: name }, confirm: 'Rename' })
    if (answer && answer.text !== name) await run('The branch was not renamed', where => haunt.renameBranch(where, name, answer.text))
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
      title: `Delete ${name}?`,
      description: lost
        ? `${lost === 1 ? 'One commit' : `${lost} commits`} on it ${lost === 1 ? 'is' : 'are'} on no other branch or tag. Git keeps ${lost === 1 ? 'it' : 'them'} for a while in its reflog, but Haunt cannot show ${lost === 1 ? 'it' : 'them'} after this.`
        : 'Its commits are on other branches too: nothing is lost.',
      confirm: 'Delete branch',
      danger: lost > 0
    })
    if (ok) await run('The branch was not deleted', where => haunt.deleteBranch(where, name, lost > 0))
  }

  async function stash(): Promise<void> {
    const answer = await dialog.prompt({
      title: 'Stash local changes',
      input: { label: 'Message', value: '', placeholder: 'What you were doing (optional)', required: false },
      checkbox: { label: 'Untracked files too', value: true },
      confirm: 'Stash'
    })
    if (answer) await run('Nothing was stashed', where => haunt.stash(where, answer.text || null, answer.checked))
  }

  const applyStash = (index: number, pop: boolean) => run(pop ? 'The stash was not popped' : 'The stash was not applied', where => haunt.applyStash(where, index, pop))

  async function dropStash(index: number, message: string): Promise<void> {
    const ok = await dialog.confirm({ title: 'Drop this stash?', description: `${message}. It is kept for 30 days and can be restored from Stashes.`, confirm: 'Drop', danger: true })
    if (!ok || !await run('The stash was not dropped', where => haunt.dropStash(where, index))) return
    const at = repo.value
    const kept = at ? await haunt.droppedStashes(at).then(list => list[0]).catch(() => undefined) : undefined
    if (kept) {
      toast.add({
        title: 'Stash dropped',
        description: `${message}. Kept for 30 days.`,
        icon: 'i-lucide-archive-x',
        duration: 12000,
        actions: [{ label: 'Undo', color: 'neutral', variant: 'outline', onClick: () => { void restoreStash(kept.id) } }]
      })
    }
  }

  const restoreStash = (id: string) => run('The stash was not brought back', where => haunt.restoreStash(where, id))

  async function newTag(at: { rev: string, label: string } | null = null): Promise<void> {
    const answer = await dialog.prompt({
      title: 'New tag',
      description: at ? `On ${at.label}.` : 'On the commit you are on.',
      input: { label: 'Name', value: '', placeholder: 'v1.2.0' },
      note: { label: 'Message (optional)', value: '', placeholder: 'Makes an annotated tag' },
      confirm: 'Create tag'
    })
    if (answer) await run('The tag was not created', where => haunt.createTag(where, answer.text, at?.rev ?? null, answer.note || null))
  }

  async function deleteTag(name: string): Promise<void> {
    const ok = await dialog.confirm({ title: `Delete the tag ${name}?`, description: 'Only here: a tag already pushed stays on the remote.', confirm: 'Delete tag', danger: true })
    if (ok) await run('The tag was not deleted', where => haunt.deleteTag(where, name))
  }

  return { busy, dropped, loadDropped, newBranch, switchTo, renameBranch, deleteBranch, stash, applyStash, dropStash, restoreStash, newTag, deleteTag }
}
