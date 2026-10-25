/** What went wrong, for people: Haunt's own message (Rust sends a sentence), else `fallback`. */
export function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error) return error
  if (error instanceof Error && error.message) return error.message
  return fallback
}
