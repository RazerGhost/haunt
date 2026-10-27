/** What went wrong, for people: Haunt's own message in the window's language (Rust sends it as `said`), else `fallback`. */
export function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error) return said(error)
  if (error instanceof Error && error.message) return said(error.message)
  return fallback
}
