import type { Worktree } from '~/bindings'

/*
 * The repository's worktrees (src-tauri/src/git/write/worktrees.rs): more branches checked out side by side, each in a
 * folder of its own, opened in tabs of their own. A new one goes beside the repository's folder by default
 * (`<folder>-<branch>`), and opens in a tab once it is made; removing one takes its folder away (its branch stays),
 * closing its tab first, and one with changes not committed asks again first. The worktree on screen is removed from
 * another tab.
 */

const list = shallowRef<Worktree[]>([])

export function useWorktrees() {
  const { repo, info, gitChanged, chooseRoot, closeTab } = useRepositories()
  const dialog = useDialog()
  const failure = useFailure()
  const toast = useToast()

  async function load(): Promise<void> {
    if (!repo.value) return
    try {
      list.value = await haunt.worktrees(repo.value)
    } catch {
      list.value = []
    }
  }

  /** The folder a new worktree for `branch` goes in by default: beside the repository's own, named after both. */
  function suggested(branch: string): string {
    const main = list.value.find(worktree => worktree.main)?.path ?? repo.value ?? ''
    const separator = main.includes('\\') ? '\\' : '/'
    const parent = main.slice(0, main.lastIndexOf(separator))
    const name = main.slice(main.lastIndexOf(separator) + 1)
    return `${parent}${separator}${name}-${branch.replace(/[\\/:*?"<>|\s]+/g, '-')}`
  }

  /** A new worktree: for `branch` (an existing local branch), or asked for (a new or existing branch). */
  async function add(branch?: string): Promise<void> {
    const at = repo.value
    if (!at) return
    let name = branch
    let isNew = false
    if (!name) {
      const answer = await dialog.prompt({
        title: 'New worktree',
        description: 'A branch in its own folder and tab: an existing one, or a new one from here.',
        input: { label: 'Branch', value: '', placeholder: 'feature/name' },
        confirm: 'Next'
      })
      name = answer?.text.trim()
      if (!name) return
      const branches = (await haunt.refs(at).catch(() => null))?.branches.map(item => item.name) ?? []
      isNew = !branches.includes(name)
    }
    const place = await dialog.prompt({
      title: `Check out ${name} in a new worktree`,
      description: isNew ? `${name} is made where you are now, and checked out in this folder.` : 'Its own folder: one that does not exist yet, or an empty one.',
      input: { label: 'Folder', value: suggested(name), placeholder: suggested(name) },
      confirm: 'Make it and open it'
    })
    const folder = place?.text.trim()
    if (!folder) return
    try {
      const made = await haunt.addWorktree(at, folder, name, isNew, null)
      await load()
      await chooseRoot(made)
      toast.add({ title: `${name} is open in its own worktree`, description: made, icon: 'i-lucide-git-fork', color: 'success' })
    } catch (error) {
      failure.failed('The worktree was not made', error, 'Git could not make it.')
    }
  }

  /** Removes a worktree's folder, asking first, and again when it has changes not committed. */
  async function remove(worktree: Worktree): Promise<void> {
    const at = repo.value
    if (!at) return
    const ok = await dialog.confirm({
      title: `Remove the worktree ${folderName(worktree.path)}?`,
      description: `Its folder (${worktree.path}) goes${worktree.branch ? `; the branch ${worktree.branch} stays` : ''}.`,
      confirm: 'Remove',
      danger: true
    })
    if (!ok) return
    // Open in a tab of its own: that tab closes first, so nothing holds the folder.
    if (info.value?.tabs.includes(worktree.path)) await closeTab(worktree.path)
    try {
      await haunt.removeWorktree(at, worktree.path, false)
    } catch (error) {
      const said = errorMessage(error, '')
      if (!said.includes('not committed')) return failure.failed('The worktree was not removed', error, 'Git could not remove it.')
      const anyway = await dialog.confirm({
        title: 'It has changes not committed',
        description: `${said} Removing it throws them away for good.`,
        confirm: 'Remove with its changes',
        danger: true
      })
      if (!anyway) return
      try {
        await haunt.removeWorktree(at, worktree.path, true)
      } catch (again) {
        return failure.failed('The worktree was not removed', again, 'Git could not remove it.')
      }
    }
    await load()
  }

  async function prune(): Promise<void> {
    if (!repo.value) return
    try {
      await haunt.pruneWorktrees(repo.value)
      await load()
    } catch (error) {
      failure.failed('Not pruned', error, 'Git could not prune them.')
    }
  }

  return { list, load, add, remove, prune, open: chooseRoot, changed: gitChanged('refs', 'head') }
}
