/*
 * Release notes between two tags, as Markdown to edit: the commits grouped by the part of the code they changed most,
 * and under each part what its functions gained, changed and lost (shared/commitDraft.ts's phrases). Worked out here,
 * without AI.
 */
import { functionPhrases, partLabel, type DraftFile } from './commitDraft.ts'
import { addTo } from './collect.ts'

export interface NoteCommit {
  sha: string
  subject: string
  paths: string[]
}

/** The part a commit belongs to: where most of its files are; null when none is in a part. */
function partOf(commit: NoteCommit, partOfPath: (path: string) => string | null): string | null {
  const count = new Map<string, number>()
  for (const path of commit.paths) {
    const part = partOfPath(path)
    if (part) count.set(part, (count.get(part) ?? 0) + 1)
  }
  return [...count].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0]?.[0] ?? null
}

export function releaseNotes(options: { from: string | null, to: string, commits: NoteCommit[], files: DraftFile[], partOfPath: (path: string) => string | null }): string {
  const { from, to, commits, files, partOfPath } = options
  const groups = new Map<string | null, NoteCommit[]>()
  for (const commit of commits) {
    const part = partOf(commit, partOfPath)
    addTo(groups, part, commit)
  }
  const functionsOf = new Map<string, DraftFile['functions']>()
  for (const file of files) if (file.part) addTo(functionsOf, file.part, ...file.functions)
  // Parts with commits first, the most commits first; then parts whose functions changed through commits counted elsewhere.
  const parts = [...groups.keys()].filter((part): part is string => part !== null)
    .sort((a, b) => groups.get(b)!.length - groups.get(a)!.length || partLabel(a).localeCompare(partLabel(b)))
  for (const part of [...functionsOf.keys()].sort()) if (!parts.includes(part) && functionPhrases(functionsOf.get(part)!).length) parts.push(part)

  const out = [`## ${from ? `${from} to ${to}` : to}`, '', `${commits.length} ${commits.length === 1 ? 'commit' : 'commits'}.`]
  for (const part of parts) {
    out.push('', `### ${partLabel(part)}`, '')
    const phrases = functionPhrases(functionsOf.get(part) ?? [], 4)
    if (phrases.length) out.push(`${phrases.join('; ').replace(/^./, first => first.toUpperCase())}.`, '')
    for (const commit of groups.get(part) ?? []) out.push(`- ${commit.subject} (${commit.sha.slice(0, 7)})`)
  }
  const other = groups.get(null) ?? []
  if (other.length) {
    out.push('', '### Other', '')
    for (const commit of other) out.push(`- ${commit.subject} (${commit.sha.slice(0, 7)})`)
  }
  return `${out.join('\n').replace(/\n{3,}/g, '\n\n')}\n`
}
