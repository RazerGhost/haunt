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
      title: t('composables.useFailure.trustTitle'),
      description: t('composables.useFailure.trustDescription'),
      confirm: t('composables.useFailure.trustConfirm')
    })
    if (!ok) return
    const problem = await setTrust(true)
    toast.add(problem
      ? { title: t('composables.useFailure.notTrusted'), description: problem, color: 'error', icon: 'i-lucide-circle-alert' }
      : { title: t('composables.useFailure.trusted'), description: t('composables.useFailure.tryNow'), icon: 'i-lucide-shield-check' })
  }

  /** Shows `error` (or `fallback` when it says nothing) under `title`. */
  function failed(title: string, error: unknown, fallback = t('composables.useFailure.fallback')): void {
    const description = errorMessage(error, fallback)
    const untrusted = /trusted folder|Trust this folder/i.test(description)
    toast.add({
      title,
      description,
      color: 'error',
      icon: 'i-lucide-circle-alert',
      duration: 10_000,
      actions: [
        ...untrusted ? [{ label: t('composables.useFailure.trustAction'), color: 'neutral' as const, variant: 'outline' as const, onClick: () => { void trust() } }] : [],
        { label: t('common.copy'), color: 'neutral' as const, variant: 'ghost' as const, onClick: () => { void navigator.clipboard?.writeText(`${title}\n${description}`) } }
      ]
    })
  }

  return { failed }
}
