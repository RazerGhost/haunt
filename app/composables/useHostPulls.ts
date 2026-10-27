import type { HostPull, HostPulls } from '~/bindings'

/*
 * Pull request status per branch, through the host's own tool (src-tauri/src/hosts.rs: gh for GitHub, glab for GitLab,
 * tea for Gitea, which is untested: no Gitea was at hand), when switched on in Settings (Git) and the tool is installed
 * and signed in; without it nothing shows.
 * Read when a repository is shown and when its refs change, at most once a minute; the sidebar marks each branch with
 * its pull request and checks, and the Pull requests screen names them. One copy for the app; plugins/haunt.client.ts
 * starts it.
 */

const found = shallowRef<HostPulls | null>(null)
/** Whether the switch is on, as Settings last said. */
const enabled = ref(false)
const EVERY_MS = 60_000

export function startHostPulls(): void {
  const { repo, gitChanged } = repositories
  const changed = gitChanged('refs')
  const ask = latest(repo)
  let last = { repo: '', at: 0 }
  async function load(force = false): Promise<void> {
    const at = repo.value
    if (!at || !enabled.value) {
      found.value = null
      return
    }
    if (!force && last.repo === at && Date.now() - last.at < EVERY_MS) return
    last = { repo: at, at: Date.now() }
    try {
      const answer = await ask.run(where => haunt.hostPulls(where))
      if (answer !== undefined) found.value = answer
    } catch { /* tried again on the next change */ }
  }
  void haunt.appSettings().then((settings) => {
    enabled.value = settings.hostTools
    void load(true)
  }).catch(() => {})
  watch(repo, () => {
    found.value = null
    void load(true)
  })
  watch(changed, () => void load())
  watch(enabled, () => void load(true))
}

export function useHostPulls() {
  const { repo } = useRepositories()
  /** By branch: its open (or draft) pull request, else its newest one. */
  const byBranch = computed(() => {
    const out = new Map<string, HostPull>()
    for (const pull of found.value?.pulls ?? []) {
      const kept = out.get(pull.branch)
      const live = (item: HostPull) => item.state === 'open' || item.state === 'draft'
      if (!kept || (live(pull) && !live(kept)) || (live(pull) === live(kept) && pull.number > kept.number)) out.set(pull.branch, pull)
    }
    return out
  })
  const byNumber = computed(() => new Map((found.value?.pulls ?? []).map(pull => [pull.number, pull])))
  function openPull(pull: HostPull): void {
    if (repo.value) void haunt.openOnWeb(repo.value, pull.url).catch(() => {})
  }
  /** Said beside what tea answers: Gitea's support has not been tried against a real server. */
  const untested = computed(() => (found.value?.tool === 'tea' ? ` ${t('composables.useHostPulls.untested')}` : ''))
  return { found, enabled, byBranch, byNumber, openPull, untested }
}

// The words the tools answer with, in the window's language; one Haunt does not know stays as the tool said it.
const WORDS = {
  state: ['open', 'draft', 'merged', 'closed'],
  checks: ['passing', 'failing', 'pending'],
  review: ['approved', 'changes requested', 'review required']
} as const
function word(group: keyof typeof WORDS, value: string): string {
  return (WORDS[group] as readonly string[]).includes(value) ? t(`composables.useHostPulls.${group}.${value.replace(/ (\w)/, (_, letter: string) => letter.toUpperCase())}`) : value
}

/** A pull request's state (open, draft, merged, closed) in words, for a badge. */
export function pullState(pull: HostPull): string {
  return word('state', pull.state)
}

/** A pull request's state and checks in words, for a tooltip. */
export function describePull(pull: HostPull): string {
  const parts = [`#${pull.number} ${pull.title}`, pull.state === 'draft' ? t('composables.useHostPulls.aDraft') : pullState(pull)]
  if (pull.checks) parts.push(t('composables.useHostPulls.checksAre', { checks: word('checks', pull.checks) }))
  if (pull.review) parts.push(word('review', pull.review))
  return parts.join(' · ')
}
