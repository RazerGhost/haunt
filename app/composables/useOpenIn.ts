/*
 * Opening a repository's folder in the editor picked in Settings, General: Open in, in the toolbar (Git and Code alike),
 * a tab's menu and the palette, and Ctrl Shift A. A function's file at its line opens through useCode instead; any file
 * of the repository (a changed one, one of a commit) through `openFile`, with `fileItems` for a file's menu.
 */
import type { ContextMenuItem } from '@nuxt/ui'
import type { Editor } from '~~/shared/types'

const NAMES: Record<Exclude<Editor, 'custom'>, string> = {
  vscode: 'VS Code', vscodium: 'VSCodium', cursor: 'Cursor', windsurf: 'Windsurf', zed: 'Zed', webstorm: 'WebStorm', sublime: 'Sublime Text', kate: 'Kate'
}

/** An editor's name, as Settings lists it. */
export function editorName(editor: Editor): string {
  return editor === 'custom' ? t('SettingsGeneral.customEditor') : NAMES[editor]
}

export function useOpenIn() {
  const { editor } = usePreferences()
  const toast = useToast()
  /** "Open in Zed", or "Open in editor" for the custom one. */
  const label = computed(() => editor.value === 'custom' ? t('composables.useOpenIn.openInEditor') : t('composables.useOpenIn.openIn', { editor: NAMES[editor.value] }))

  async function open(path: string): Promise<void> {
    try {
      await haunt.openFolderTab(path, editor.value)
    } catch (error) {
      toast.add({ title: t('composables.useOpenIn.notOpened'), description: errorMessage(error, t('composables.useCode.tryAnotherEditor')), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }

  const { repo } = useRepositories()
  /** Opens a file of the shown repository, by its path in it, at `line`. */
  async function openFile(path: string, line = 1): Promise<void> {
    if (!repo.value) return
    try {
      await haunt.openRepoFile(repo.value, editor.value, path, line)
    } catch (error) {
      toast.add({ title: t('composables.useOpenIn.notOpened'), description: errorMessage(error, t('composables.useCode.tryAnotherEditor')), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }
  async function revealFile(path: string): Promise<void> {
    if (!repo.value) return
    try {
      await haunt.revealRepoFile(repo.value, path)
    } catch (error) {
      toast.add({ title: t('composables.useOpenIn.notShown'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
    }
  }
  /** The file's full path, in the system's form. */
  function copyPath(path: string): void {
    const root = repo.value ?? ''
    const windows = root.includes('\\')
    void navigator.clipboard?.writeText(`${root}${windows ? '\\' : '/'}${windows ? path.replaceAll('/', '\\') : path}`)
  }
  /** A file's menu items: open it in the editor and show it in the file manager (when it is there now), copy its path. */
  function fileItems(path: string, there = true, enter = false): ContextMenuItem[] {
    return [
      { label: label.value, icon: 'i-lucide-square-arrow-out-up-right', kbds: enter ? ['enter'] : undefined, disabled: !there, onSelect: () => openFile(path) },
      { label: t('composables.useOpenIn.showInFolder'), icon: 'i-lucide-folder-open', disabled: !there, onSelect: () => revealFile(path) },
      { label: t('composables.useOpenIn.copyPath'), icon: 'i-lucide-copy', onSelect: () => copyPath(path) }
    ]
  }

  return { label, open, openFile, revealFile, copyPath, fileItems }
}
