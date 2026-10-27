import type { RemoteInfo } from '~/bindings'

/*
 * The repository's remotes, from the sidebar: add one, rename it, change its URL, copy it, remove it. Each asks what it
 * needs (a name, a URL, a yes before its remote branches go), runs the write in Rust (git/write/remotes.rs) and says
 * what went wrong, in git's words. The list (with the URLs) comes from the repository's overview, read when the
 * repository changes and after each write here; the sidebar's refs are read again on the git news Rust sends.
 */

const list = shallowRef<RemoteInfo[]>([])

export function useRemotes() {
  const { repo } = useRepositories()
  const { refs } = useRefs()
  const dialog = useDialog()
  const failure = useFailure()

  async function load(): Promise<void> {
    const at = repo.value
    if (!at) return
    try {
      const overview = await haunt.repositoryOverview(at)
      // Another tab may be on screen by now.
      if (repo.value === at) list.value = overview.remotes
    } catch {
      list.value = []
    }
  }

  async function run(what: string, write: (at: string) => Promise<unknown>): Promise<boolean> {
    const at = repo.value
    if (!at) return false
    try {
      await write(at)
      return true
    } catch (error) {
      failure.failed(what, error)
      return false
    } finally {
      void load()
    }
  }

  /** The URL of `name` as git has it now; null when it has none. */
  async function urlOf(name: string): Promise<string | null> {
    await load()
    return list.value.find(remote => remote.name === name)?.url ?? null
  }

  /** A new remote: its name (origin when there is none yet), then its URL. */
  async function add(): Promise<void> {
    await load()
    const named = await dialog.prompt({
      title: t('composables.useRemotes.newRemote'),
      input: { label: t('composables.useRemotes.name'), value: list.value.length ? '' : 'origin', placeholder: 'upstream' },
      confirm: t('composables.useRemotes.next')
    })
    if (!named) return
    const answer = await dialog.prompt({
      title: t('composables.useRemotes.newRemote'),
      description: t('composables.useRemotes.urlFor', { name: named.text }),
      input: { label: t('composables.useRemotes.url'), value: '', placeholder: 'https://github.com/owner/repository.git' },
      confirm: t('composables.useRemotes.addRemote')
    })
    if (answer) await run(t('composables.useRemotes.notAdded'), where => haunt.addRemote(where, named.text, answer.text))
  }

  async function rename(name: string): Promise<void> {
    const answer = await dialog.prompt({
      title: t('composables.useRemotes.renameTitle', { name }),
      description: t('composables.useRemotes.renameHelp'),
      input: { label: t('composables.useRemotes.newName'), value: name },
      confirm: t('composables.useRemotes.rename')
    })
    if (answer && answer.text !== name) await run(t('composables.useRemotes.notRenamed'), where => haunt.renameRemote(where, name, answer.text))
  }

  async function changeUrl(name: string): Promise<void> {
    const url = await urlOf(name)
    const answer = await dialog.prompt({
      title: t('composables.useRemotes.changeUrlTitle', { name }),
      input: { label: t('composables.useRemotes.url'), value: url ?? '', placeholder: 'https://github.com/owner/repository.git' },
      confirm: t('composables.useRemotes.changeUrl')
    })
    if (answer && answer.text !== url) await run(t('composables.useRemotes.urlNotChanged'), where => haunt.setRemoteUrl(where, name, answer.text))
  }

  async function copyUrl(name: string): Promise<void> {
    const url = await urlOf(name)
    if (!url) return
    try {
      await navigator.clipboard.writeText(url)
    } catch { /* no clipboard: nothing to do */ }
  }

  /** Removes a remote after saying its remote branches go too. */
  async function remove(name: string): Promise<void> {
    const count = refs.value?.remotes.find(remote => remote.name === name)?.branches.length ?? 0
    const ok = await dialog.confirm({
      title: t('composables.useRemotes.removeTitle', { name }),
      description: count
        ? t('composables.useRemotes.removeHelp', { count })
        : t('composables.useRemotes.removeHelpNone'),
      confirm: t('composables.useRemotes.removeRemote'),
      danger: true
    })
    if (ok) await run(t('composables.useRemotes.notRemoved'), where => haunt.removeRemote(where, name))
  }

  return { list, load, add, rename, changeUrl, copyUrl, remove }
}
