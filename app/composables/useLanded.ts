import type { Landed, LandedBranch } from '~/bindings'

/*
 * What the main line (origin/main) already has of the other branches, under other shas: commits cherry-picked or
 * rebased in, and branches squash-merged (Rust's `landed`). The commit list dims those commits and can leave those
 * branches out; the sidebar and Branch cleanup offer the branches. Comparing patches reads a lot of history, so it is
 * read only once something asks for it, a moment after the refs settle, and again only when a branch moved.
 */
const result = shallowRef<Landed | null>(null)
let started = false

function start(): void {
  started = true
  effectScope(true).run(() => {
    const { repo } = useRepositories()
    const { refs } = useRefs()
    const ask = latest(repo)
    // The tips compared: nothing to read again while none of them moved.
    const key = computed(() => {
      const all = refs.value
      if (!repo.value || !all) return null
      return JSON.stringify([repo.value, all.branches.map(branch => [branch.name, branch.sha]), all.remotes.map(remote => remote.branches.map(branch => branch.sha))])
    })
    let timer: ReturnType<typeof setTimeout> | undefined
    watch(repo, () => { result.value = null })
    watch(key, (now) => {
      clearTimeout(timer)
      if (!now) return
      timer = setTimeout(async () => {
        try {
          const found = await ask.run(at => haunt.landed(at))
          if (found !== undefined) result.value = found
        } catch { /* nothing dimmed */ }
      }, 1500)
    }, { immediate: true })
  })
}

export function useLanded() {
  if (!started) start()
  const commits = computed(() => new Map((result.value?.commits ?? []).map(commit => [commit.sha, commit.landedAs])))
  const base = computed(() => result.value?.base ?? null)
  /** The branches and remote branches the main line has in full. */
  const whole = computed(() => (result.value?.branches ?? []).filter(branch => branch.landed === branch.own))
  /** A local branch's entry, by its short name. */
  const branch = (name: string): LandedBranch | undefined => result.value?.branches.find(one => one.reference === `refs/heads/${name}`)
  return { commits, base, whole, branch }
}
