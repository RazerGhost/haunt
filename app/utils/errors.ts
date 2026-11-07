/** Whether a write failed on an old .git/index.lock, which the window then offers to remove (`removeStaleLock`). */
export function staleLock(error: unknown): boolean {
  const raw = typeof error === 'string' ? error : error instanceof Error ? error.message : ''
  return raw.includes('\u0001write.staleLock\u0001')
}

/** What went wrong, for people: Haunt's own message in the window's language (Rust sends it as `said`), else `fallback`. */
export function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error) return said(error)
  if (error instanceof Error && error.message) return said(error.message)
  return fallback
}
