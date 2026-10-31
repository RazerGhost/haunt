/*
 * Opening a repository's folder in the editor picked in Settings, General: Open in, in the toolbar (Git and Code alike),
 * a tab's menu and the palette, and Ctrl Shift A. A function's file at its line opens through useCode instead.
 */
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

  return { label, open }
}
