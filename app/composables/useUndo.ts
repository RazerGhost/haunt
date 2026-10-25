import type { LastAction } from '~/bindings'

/*
 * Undo for the last write that moved history (src-tauri/src/git/write/last.rs): after a commit, merge, pull, reset,
 * rebase, branch switch, or a branch made, renamed or deleted, Rust says what Undo would do; it shows as a toast with
 * Undo, and the palette offers it while nothing has moved since. One copy for the app; plugins/haunt.client.ts
 * starts it.
 */

const last = shallowRef<LastAction | null>(null)
let toast: ReturnType<typeof useToast> | null = null

async function undo(): Promise<void> {
  const at = repositories.repo.value
  if (!at) return
  toast?.remove('undo')
  try {
    const title = await haunt.undoLast(at)
    last.value = null
    toast?.add({ title: `Undone: ${title}`, icon: 'i-lucide-undo-2' })
  } catch (error) {
    toast?.add({ title: 'Not undone', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}

export function startUndo(): void {
  const { repo, onNews, gitChanged } = repositories
  toast = useToast()
  onNews((news) => {
    if (news.type !== 'done' || news.repo !== repo.value) return
    last.value = { title: news.title, undo: news.undo }
    toast?.add({
      id: 'undo',
      title: news.title,
      description: `Undo: ${news.undo}`,
      duration: 12000,
      icon: 'i-lucide-check',
      actions: [{ label: 'Undo', color: 'neutral', variant: 'outline', onClick: () => { void undo() } }]
    })
  })
  // Any other change in git may leave nothing to undo: asked again.
  watch([repo, gitChanged('head', 'refs')], async () => {
    const at = repo.value
    const found = at ? await haunt.lastAction(at).catch(() => null) : null
    if (repo.value === at) last.value = found
  }, { immediate: true })
}

export function useUndo() {
  return { last, undo }
}
