import type { PatchPreview } from '~/bindings'

/*
 * Patches (src-tauri/src/git/write/patches.rs): commits saved as patch files in a folder you pick, and patch files
 * applied after a preview of what they change function by function (app/pages/patches.vue). The files chosen and their
 * preview are kept while the app runs, so leaving the screen loses nothing.
 */

const files = ref<string[]>([])
const preview = shallowRef<PatchPreview | null>(null)

export function usePatches() {
  const { repo } = useRepositories()
  const failure = useFailure()
  const toast = useToast()
  const busy = ref(false)

  /** Saves a commit (with `from`: every commit after `from` up to `to`) as patch files in a folder picked now. */
  async function save(to: string, from: string | null = null): Promise<void> {
    const at = repo.value
    if (!at) return
    const { open } = await import('@tauri-apps/plugin-dialog')
    const folder = await open({ directory: true, title: 'Save the patches in' })
    if (typeof folder !== 'string') return
    try {
      const saved = await haunt.savePatches(at, from, to, folder)
      toast.add({ title: saved.length === 1 ? 'Saved a patch' : `Saved ${saved.length} patches`, description: folder, icon: 'i-lucide-file-diff', color: 'success' })
    } catch (error) {
      failure.failed('No patch saved', error)
    }
  }

  async function read(): Promise<void> {
    const at = repo.value
    if (!at || !files.value.length) {
      preview.value = null
      return
    }
    busy.value = true
    try {
      preview.value = await haunt.previewPatches(at, files.value)
    } catch (error) {
      preview.value = null
      failure.failed('The patches could not be read', error)
    } finally {
      busy.value = false
    }
  }

  /** Picks patch files and reads them. */
  async function choose(): Promise<void> {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const chosen = await open({ multiple: true, title: 'Apply patches', filters: [{ name: 'Patches', extensions: ['patch', 'diff', 'mbox', 'eml', 'txt'] }] })
    if (!chosen) return
    files.value = (Array.isArray(chosen) ? chosen : [chosen]).sort()
    await read()
  }

  /** Applies them, as commits or as changes in the working tree; the list empties once they are in. */
  async function apply(commit: boolean): Promise<boolean> {
    const at = repo.value
    if (!at || busy.value) return false
    busy.value = true
    try {
      await haunt.applyPatches(at, files.value, commit)
      toast.add({ title: commit ? 'Patches committed' : 'Patches applied to the working tree', icon: 'i-lucide-check', color: 'success' })
      files.value = []
      preview.value = null
      return true
    } catch (error) {
      failure.failed('The patches were not applied', error)
      return false
    } finally {
      busy.value = false
    }
  }

  function clear(): void {
    files.value = []
    preview.value = null
  }

  return { files, preview, busy, save, choose, read, apply, clear }
}
