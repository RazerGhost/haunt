/*
 * Git or Code: the two halves of the window. Git is the history and the work on it (local changes, commits, conflicts,
 * a file's history, blame, rebase), with the branches, remotes, tags and stashes in the sidebar. Code is the views of
 * the code (graph, unused, routes), with the code's own sidebar, so the git lists do not crowd them. The
 * mode follows the screen shown; switching goes back to the screen last open on the other side.
 */

const CODE = ['/graph', '/unused', '/routes']
export type Mode = 'git' | 'code'

/** The screen last shown on each side (path and query), for switching back to it. */
const last = { git: '/commits', code: '/graph' } as Record<Mode, string>

function modeOf(path: string): Mode {
  return CODE.includes(path) ? 'code' : 'git'
}

export function useMode() {
  const route = useRoute()
  const router = useRouter()
  const mode = computed<Mode>(() => modeOf(route.path))
  watch(() => route.fullPath, () => { last[modeOf(route.path)] = route.fullPath }, { immediate: true })
  function switchTo(next: Mode): void {
    if (next !== mode.value) void router.push(last[next])
  }
  return { mode, switchTo }
}
