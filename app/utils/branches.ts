import type { BranchRef } from '~/bindings'

/*
 * Small judgements about branches shared by the toolbar, the sidebar and the commit list.
 */

/** A branch and its remote branch went far apart: both have many commits the other has not (often the remote's history
 * was rewritten), so a pull would merge or rebase a lot. A few each way is an ordinary pull. */
export function wentApart(branch: Pick<BranchRef, 'upstream' | 'gone' | 'ahead' | 'behind'>): boolean {
  return !!branch.upstream && !branch.gone && branch.ahead > 0 && branch.behind > 0 && branch.ahead + branch.behind >= 20
}

/** A long name cut in the middle (`feature/very-…-login`), so both its start and its end stay readable. */
export function middle(text: string, max = 24): string {
  if (text.length <= max) return text
  const keep = max - 1
  const start = Math.ceil(keep / 2)
  return `${text.slice(0, start)}…${text.slice(text.length - (keep - start))}`
}

/** A name's initials, for a narrow author column: "Ada Lovelace" is AL, "ada" is A. */
export function initials(name: string): string {
  const words = name.trim().split(/[\s._-]+/).filter(Boolean)
  return (words.length > 1 ? `${words[0]![0]}${words.at(-1)![0]}` : (words[0]?.[0] ?? '?')).toUpperCase()
}
