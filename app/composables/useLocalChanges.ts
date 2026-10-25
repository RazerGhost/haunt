import type { Activity, CommitBox, Discarded, LineRef, LinesAction, LocalStatus } from '~/bindings'

/*
 * Local changes as files: the staged and unstaged files, the saved discards, what the commit box needs to
 * know, and what Haunt ran to write. Read when a repository is shown, on git news (a write, a commit, staging in your
 * own terminal), after a scan (a saved file) and when the window gets the focus again (an edit Haunt did not see). The
 * writes go through Rust (git/write), which saves a discard first. One copy for the app; plugins/haunt.client.ts
 * starts it.
 */

/** Null until read; then the lists, or null when the folder is not in a git repository. */
const result = shallowRef<{ status: LocalStatus | null } | null>(null)
const discarded = shallowRef<Discarded[]>([])
const commitBox = shallowRef<CommitBox | null>(null)
const activity = shallowRef<Activity[]>([])
const busy = ref(false)
const problem = ref('')

export function startLocalChanges(): void {
  const { repo, gitChanged } = repositories
  const { scan } = useCode()
  const changed = gitChanged('head', 'index', 'refs')
  const ask = latest(repo)
  let timer: ReturnType<typeof setTimeout> | null = null

  async function load(): Promise<void> {
    const at = repo.value
    try {
      // All at once: each starts a few git processes.
      const found = await ask.run(at => Promise.all([
        haunt.localStatus(at),
        haunt.discarded(at).catch(() => []),
        haunt.commitBox(at).catch(() => null),
        haunt.activity(at).catch(() => [])
      ]))
      if (found === undefined) return
      result.value = { status: found[0] }
      discarded.value = found[1]
      commitBox.value = found[2]
      activity.value = found[3]
    } catch {
      // Tried again soon while nothing was read yet (Rust may still be starting); after that, on the next event.
      if (!result.value) setTimeout(() => repo.value === at && void load(), 2000)
    }
  }
  function later(): void {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void load(), 150)
  }

  watch(repo, (now) => {
    result.value = null
    discarded.value = []
    commitBox.value = null
    activity.value = []
    problem.value = ''
    ask.cancel()
    if (now) later()
  }, { immediate: true })
  watch([changed, () => scan.value?.scannedAt], () => repo.value && later())
  window.addEventListener('focus', () => repo.value && later())
}

export function useLocalChanges() {
  const { repo } = repositories
  const toast = useToast()
  const status = computed(() => result.value?.status ?? null)
  const notRepository = computed(() => result.value !== null && result.value.status === null)

  /**
   * Runs a write; its error is shown. A staging or discarding write answers with the lists after it, shown at once;
   * the rest (the saved discards, the activity) is read again on the git news Rust sends after every write.
   */
  async function run(write: (repo: string) => Promise<unknown>): Promise<boolean> {
    if (!repo.value || busy.value) return false
    const at = repo.value
    busy.value = true
    problem.value = ''
    try {
      const answer = await write(at)
      if (answer !== null && typeof answer === 'object' && 'staged' in answer && repo.value === at) result.value = { status: answer as LocalStatus }
      return true
    } catch (error) {
      problem.value = errorMessage(error, 'Git could not do that.')
      return false
    } finally {
      busy.value = false
    }
  }

  /** After a discard: a toast whose Undo brings back what was just saved (the newest of Recently discarded). */
  async function discardWithUndo(what: string, write: (repo: string) => Promise<unknown>): Promise<boolean> {
    const done = await run(write)
    const at = repo.value
    if (!done || !at) return done
    const saved = await haunt.discarded(at).then(list => list[0]).catch(() => undefined)
    if (saved) {
      toast.add({
        title: what,
        description: 'Saved first: it can be brought back for 30 days.',
        icon: 'i-lucide-undo-2',
        duration: 12000,
        actions: [{ label: 'Undo', color: 'neutral', variant: 'outline', onClick: () => { void run(where => haunt.restore(where, saved.id)) } }]
      })
    }
    return done
  }

  return {
    status,
    notRepository,
    discarded,
    commitBox,
    activity,
    busy,
    problem,
    stage: (paths: string[]) => run(at => haunt.stage(at, paths)),
    unstage: (paths: string[]) => run(at => haunt.unstage(at, paths)),
    lines: (path: string, picked: LineRef[], action: LinesAction) => action === 'discard'
      ? discardWithUndo(`Discarded ${picked.length} ${picked.length === 1 ? 'line' : 'lines'}`, at => haunt.lines(at, path, picked, action))
      : run(at => haunt.lines(at, path, picked, action)),
    discard: (paths: string[]) => discardWithUndo(paths.length === 1 ? `Discarded ${paths[0]}` : `Discarded ${paths.length} files`, at => haunt.discard(at, paths)),
    restore: (id: string) => run(at => haunt.restore(at, id)),
    commit: (message: string, amend: boolean, signoff: boolean) => run(at => haunt.commitStaged(at, message, amend, signoff))
  }
}
