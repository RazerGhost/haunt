import type { WebRemote } from '~/bindings'
import { branchLink, commitLink, fileLink, hostName } from '~~/shared/weblinks'

/*
 * The shown repository's remote on the web (GitHub, GitLab, Bitbucket, Gitea; git/remote.rs), read when a repository
 * is shown and when its refs change, and the menus' "Open on GitHub" and "Copy link": a commit, a file at a commit
 * and line (a function's lines), or a branch, opened in the browser or copied. Rust opens only pages of this remote. One copy for the app; plugins/haunt.client.ts
 * starts it.
 */

const web = shallowRef<WebRemote | null>(null)

export function startWeb(): void {
  const { repo, gitChanged } = repositories
  const ask = latest(repo)
  async function load(): Promise<void> {
    try {
      const found = await ask.run(at => haunt.webRemote(at))
      if (found !== undefined) web.value = found
    } catch { web.value = null }
  }
  watch(repo, () => { web.value = null })
  watch([repo, gitChanged('refs')], () => repo.value && void load(), { immediate: true })
}

export function useWeb() {
  const { repo } = useRepositories()
  const toast = useToast()
  /** "Open on GitHub", or null when the repository has no remote on the web. */
  const label = computed(() => (web.value ? `Open on ${hostName(web.value.kind)}` : null))
  /** "Copy GitHub link", or null. */
  const copyLabel = computed(() => (web.value ? `Copy ${hostName(web.value.kind)} link` : null))

  async function copy(url: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(url)
      toast.add({ title: 'Link copied', description: url, icon: 'i-lucide-link' })
    } catch {
      toast.add({ title: 'Could not copy the link', description: url, color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }

  async function open(url: string): Promise<void> {
    if (!repo.value) return
    try {
      await haunt.openOnWeb(repo.value, url)
    } catch (error) {
      toast.add({ title: 'Could not open the page', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }
  return {
    web,
    label,
    copyLabel,
    openCommit: (sha: string) => web.value && open(commitLink(web.value, sha)),
    openFile: (at: string, path: string, line?: number, endLine?: number) => web.value && open(fileLink(web.value, at, path, line, endLine)),
    openBranch: (name: string) => web.value && open(branchLink(web.value, name)),
    copyCommit: (sha: string) => web.value && copy(commitLink(web.value, sha)),
    copyFile: (at: string, path: string, line?: number, endLine?: number) => web.value && copy(fileLink(web.value, at, path, line, endLine)),
    copyBranch: (name: string) => web.value && copy(branchLink(web.value, name))
  }
}
