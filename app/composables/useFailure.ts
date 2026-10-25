/*
 * A toast for what went wrong, in git's words, with what helps next: Copy (to paste it somewhere), and, when the
 * write was refused because the folder is not trusted, Trust this folder (asked first, saying what trusting means),
 * instead of finding the shield in the toolbar. Every command is also in Activity, in Local changes.
 */
export function useFailure() {
  const toast = useToast()
  const dialog = useDialog()
  const { setTrust } = useRepositories()

  async function trust(): Promise<void> {
    const ok = await dialog.confirm({
      title: 'Trust this folder?',
      description: 'Haunt then also runs the folder\'s own code: its TypeScript and architecture script while scanning, and its git hooks and filters when writing. Pull and push run only in a trusted folder.',
      confirm: 'Trust it'
    })
    if (!ok) return
    const problem = await setTrust(true)
    toast.add(problem
      ? { title: 'Not trusted', description: problem, color: 'error', icon: 'i-lucide-circle-alert' }
      : { title: 'This folder is trusted', description: 'Try again now.', icon: 'i-lucide-shield-check' })
  }

  /** Shows `error` (or `fallback` when it says nothing) under `title`. */
  function failed(title: string, error: unknown, fallback = 'Git could not do that.'): void {
    const description = errorMessage(error, fallback)
    const untrusted = /trusted folder|Trust this folder/i.test(description)
    toast.add({
      title,
      description,
      color: 'error',
      icon: 'i-lucide-circle-alert',
      duration: 10_000,
      actions: [
        ...untrusted ? [{ label: 'Trust this folder…', color: 'neutral' as const, variant: 'outline' as const, onClick: () => { void trust() } }] : [],
        { label: 'Copy', color: 'neutral' as const, variant: 'ghost' as const, onClick: () => { void navigator.clipboard?.writeText(`${title}\n${description}`) } }
      ]
    })
  }

  return { failed }
}
