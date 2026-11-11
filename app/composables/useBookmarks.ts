
/*
 * Bookmarked files: the ones you come back to, kept per repository between launches, a click away in the Code sidebar
 * and the palette, next to the pinned functions (usePins). A bookmark is a file's path; one the scan no longer has
 * shows as gone until it is removed. Opening one shows its graph.
 */
export function useBookmarks() {
  const { repo } = useRepositories()
  const { structure: scan } = useCode()
  const router = useRouter()
  const all = usePersisted<Record<string, string[]>>('bookmarks', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
  const paths = computed(() => (repo.value ? all.value[repo.value] ?? [] : []))
  const byPath = computed(() => new Map((scan.value?.files ?? []).map(file => [file.path, file])))
  /** The bookmarked files as the scan has them now; `found` is null for one that is gone. */
  const bookmarks = computed(() => paths.value.map(path => ({ path, found: byPath.value.get(path) ?? null })))

  const isBookmarked = (path: string) => paths.value.includes(path)
  function toggle(path: string): void {
    if (!repo.value) return
    const rest = paths.value.filter(other => other !== path)
    all.value = { ...all.value, [repo.value]: rest.length === paths.value.length ? [...rest, path] : rest }
  }
  function remove(path: string): void {
    if (repo.value) all.value = { ...all.value, [repo.value]: paths.value.filter(other => other !== path) }
  }
  /** Shows the file's graph: its functions, and the files they call and are called from. */
  function show(path: string): Promise<unknown> | undefined {
    if (!byPath.value.has(path)) return
    return router.push({ path: '/graph', query: { file: path } })
  }
  return { bookmarks, isBookmarked, toggle, remove, show }
}
