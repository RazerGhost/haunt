/*
 * A toast for what went wrong, in git's words, with what helps next: Copy (to paste it somewhere), and, when the
 * write was refused because the folder is not trusted, Trust this folder (asked first, saying what trusting means),
 * instead of finding the shield in the toolbar. `trustFirst` asks the same before an action that needs a trusted folder
 * (pull, push, updating submodules, running tests), so those stay clickable in a folder not trusted yet. It stays until closed or acted on, and the same failure again does not
 * add a second copy (its id is its words). Every command is also in Activity, in Local changes.
 */
export function useFailure() {
  const toast = useToast()
  const dialog = useDialog()
  const { info, setTrust } = useRepositories()

  /** Whether the folder is trusted now: at once when it was, else after asking (false when declined or it failed). */
  async function trustFirst(): Promise<boolean> {
    if (info.value?.trusted) return true
    const ok = await dialog.confirm({
      title: t('composables.useFailure.trustTitle'),
      description: t('composables.useFailure.trustDescription'),
      confirm: t('composables.useFailure.trustConfirm')
    })
    if (!ok) return false
    const problem = await setTrust(true)
    if (problem) show(t('composables.useFailure.notTrusted'), problem, false)
    return !problem
  }

  async function trust(): Promise<void> {
    if (await trustFirst()) toast.add({ title: t('composables.useFailure.trusted'), description: t('composables.useFailure.tryNow'), icon: 'i-lucide-shield-check' })
  }

  function show(title: string, description: string, untrusted: boolean): void {
    toast.add({
      id: `failure\n${title}\n${description}`,
      title,
      description,
      color: 'error',
      icon: 'i-lucide-circle-alert',
      duration: 0,
      close: false,
      actions: [
        ...untrusted ? [{ label: t('composables.useFailure.trustAction'), color: 'neutral' as const, variant: 'outline' as const, onClick: () => { void trust() } }] : [],
        { label: t('common.copy'), icon: 'i-lucide-copy', color: 'neutral' as const, variant: 'ghost' as const, closeOnClick: false, onClick: () => { void navigator.clipboard?.writeText(`${title}\n${description}`) } },
        { label: t('common.close'), color: 'neutral' as const, variant: 'ghost' as const }
      ]
    })
  }

  /** Shows `error` (or `fallback` when it says nothing) under `title`. */
  function failed(title: string, error: unknown, fallback = t('composables.useFailure.fallback')): void {
    const description = errorMessage(error, fallback)
    show(title, description, /trusted folder|Trust this folder/i.test(description))
  }

  return { failed, trustFirst }
}
