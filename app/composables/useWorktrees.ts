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
        title: t('composables.useWorktrees.newWorktree'),
        description: t('composables.useWorktrees.newHelp'),
        input: { label: t('composables.useWorktrees.branch'), value: '', placeholder: 'feature/name' },
        confirm: t('composables.useWorktrees.next')
      })
      name = answer?.text.trim()
      if (!name) return
      const branches = (await haunt.refs(at).catch(() => null))?.branches.map(item => item.name) ?? []
      isNew = !branches.includes(name)
    }
    const place = await dialog.prompt({
      title: t('composables.useWorktrees.placeTitle', { name }),
      description: isNew ? t('composables.useWorktrees.placeNew', { name }) : t('composables.useWorktrees.placeHelp'),
      input: { label: t('composables.useWorktrees.folder'), value: suggested(name), placeholder: suggested(name) },
      confirm: t('composables.useWorktrees.make')
    })
    const folder = place?.text.trim()
    if (!folder) return
    try {
      const made = await haunt.addWorktree(at, folder, name, isNew, null)
      await load()
      await chooseRoot(made)
      toast.add({ title: t('composables.useWorktrees.opened', { name }), description: made, icon: 'i-lucide-git-fork', color: 'success' })
    } catch (error) {
      failure.failed(t('composables.useWorktrees.notMade'), error, t('composables.useWorktrees.notMadeHelp'))
    }
  }

  /** Removes a worktree's folder, asking first, and again when it has changes not committed. */
  async function remove(worktree: Worktree): Promise<void> {
    const at = repo.value
    if (!at) return
    const ok = await dialog.confirm({
      title: t('composables.useWorktrees.removeTitle', { name: folderName(worktree.path) }),
      description: worktree.branch ? t('composables.useWorktrees.removeHelpBranch', { path: worktree.path, branch: worktree.branch }) : t('composables.useWorktrees.removeHelp', { path: worktree.path }),
      confirm: t('composables.useWorktrees.remove'),
      danger: true
    })
    if (!ok) return
    // Open in a tab of its own: that tab closes first, so nothing holds the folder.
    if (info.value?.tabs.includes(worktree.path)) await closeTab(worktree.path)
    try {
      await haunt.removeWorktree(at, worktree.path, false)
    } catch (error) {
      const said = errorMessage(error, '')
      if (!said.includes('not committed')) return failure.failed(t('composables.useWorktrees.notRemoved'), error, t('composables.useWorktrees.notRemovedHelp'))
      const anyway = await dialog.confirm({
        title: t('composables.useWorktrees.uncommitted'),
        description: t('composables.useWorktrees.uncommittedHelp', { said }),
        confirm: t('composables.useWorktrees.removeAnyway'),
        danger: true
      })
      if (!anyway) return
      try {
        await haunt.removeWorktree(at, worktree.path, true)
      } catch (again) {
        return failure.failed(t('composables.useWorktrees.notRemoved'), again, t('composables.useWorktrees.notRemovedHelp'))
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
      failure.failed(t('composables.useWorktrees.notPruned'), error, t('composables.useWorktrees.notPrunedHelp'))
    }
  }

  return { list, load, add, remove, prune, open: chooseRoot, changed: gitChanged('refs', 'head') }
}
