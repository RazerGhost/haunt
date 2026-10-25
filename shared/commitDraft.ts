/*
 * A commit message drafted from what is staged, without AI: nothing leaves the machine. It says what changed by function
 * ("Entries: adds resolveModel; changes createEntry; 3 callers in api"), never why; that part is for the person
 * committing. The release notes (shared/releaseNotes.ts) describe functions the same way.
 */

export type Status = 'added' | 'changed' | 'removed' | 'moved'

export interface DraftFunction {
  name: string
  status: Status
  /** For a moved function, its name before. */
  from?: { name: string } | null
}

export interface DraftFile {
  path: string
  oldPath: string
  status: 'added' | 'modified' | 'deleted' | 'renamed'
  part: string | null
  code: boolean
  functions: DraftFunction[]
}

/** git's advice is a subject of at most 50 characters; past 72 a subject is cut off in most places. */
const SUBJECT = 72
const BODY = 72

/** A part's name for people: its last folder, capitalised (`src/features/entries` → Entries). */
export function partLabel(part: string): string {
  const last = part.split(/[\\/]/).filter(Boolean).pop() ?? part
  return last.charAt(0).toUpperCase() + last.slice(1)
}

/** Named functions only: anonymous callbacks (`(anonymous)`) carry nothing a reader can use. */
function named(functions: DraftFunction[]): DraftFunction[] {
  return functions.filter(fn => !fn.name.startsWith('('))
}

function list(names: string[], most: number, noun = 'functions'): string {
  const unique = [...new Set(names)]
  if (unique.length > most) return `${unique.length} ${noun}`
  if (unique.length <= 2) return unique.join(' and ')
  return `${unique.slice(0, -1).join(', ')} and ${unique.at(-1)}`
}

/**
 * What functions a change adds, changes, renames, moves and removes, as short phrases ("adds resolveModel", "changes 4
 * functions"); `most` names per phrase, past which they are counted.
 */
export function functionPhrases(functions: DraftFunction[], most = 2): string[] {
  const fns = named(functions)
  const of = (status: Status) => fns.filter(fn => fn.status === status)
  const out: string[] = []
  const added = of('added').map(fn => fn.name)
  if (added.length) out.push(`adds ${list(added, most)}`)
  const changed = of('changed').map(fn => fn.name)
  if (changed.length) out.push(`changes ${list(changed, most)}`)
  const moved = of('moved')
  const renamed = moved.filter(fn => fn.from && fn.from.name !== fn.name)
  if (renamed.length) {
    out.push(renamed.length > most ? `renames ${renamed.length} functions` : `renames ${renamed.map(fn => `${fn.from!.name} to ${fn.name}`).join(', ')}`)
  }
  const movedOnly = moved.filter(fn => !fn.from || fn.from.name === fn.name).map(fn => fn.name)
  if (movedOnly.length) out.push(`moves ${list(movedOnly, most)}`)
  const removed = of('removed').map(fn => fn.name)
  if (removed.length) out.push(`removes ${list(removed, most)}`)
  return out
}

const base = (path: string) => path.split('/').pop() ?? path

/** Phrases for files without functions to name (docs, config, files Haunt does not read). */
function filePhrases(files: DraftFile[], most = 2): string[] {
  const out: string[] = []
  const by = (status: DraftFile['status']) => files.filter(file => file.status === status)
  const added = by('added').map(file => base(file.path))
  if (added.length) out.push(`adds ${list(added, most, 'files')}`)
  const modified = by('modified').map(file => base(file.path))
  if (modified.length) out.push(`changes ${list(modified, most, 'files')}`)
  const renamed = by('renamed')
  if (renamed.length) out.push(renamed.length > most ? `renames ${renamed.length} files` : `renames ${renamed.map(file => `${base(file.oldPath)} to ${base(file.path)}`).join(', ')}`)
  const deleted = by('deleted').map(file => base(file.path))
  if (deleted.length) out.push(`removes ${list(deleted, most, 'files')}`)
  return out
}

/** The part most of the change is in: the most named functions, then the most files. */
function mainPart(files: DraftFile[]): string | null {
  const score = new Map<string, number>()
  for (const file of files) {
    if (!file.part) continue
    score.set(file.part, (score.get(file.part) ?? 0) + 1 + named(file.functions).length * 10)
  }
  return [...score].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0]?.[0] ?? null
}

/** Wraps a list line at `width`, the lines after the first indented under its text. */
function wrap(line: string, width = BODY): string[] {
  if (line.length <= width) return [line]
  const out: string[] = []
  let current = ''
  for (const word of line.split(' ')) {
    if (current && current.length + 1 + word.length > width) {
      out.push(current)
      current = `  ${word}`
    } else {
      current = current ? `${current} ${word}` : word
    }
  }
  if (current) out.push(current)
  return out
}

/**
 * The draft: a subject (`Part: phrases`, kept within 72 characters by counting names, then dropping phrases), and a
 * body with a line per file. `callers` counts the named functions outside the change that call into it, per part.
 */
export function draftCommitMessage(files: DraftFile[], callers: Map<string, number> = new Map()): { subject: string, body: string } {
  if (!files.length) return { subject: '', body: '' }
  const part = mainPart(files)
  const label = part ? `${partLabel(part)}: ` : ''
  const all = files.flatMap(file => file.functions)
  const plain = files.filter(file => !named(file.functions).length)
  const pick = (most: number) => [...functionPhrases(all, most), ...(named(all).length ? [] : filePhrases(plain, most))]

  let phrases = pick(2)
  if (`${label}${phrases.join('; ')}`.length > SUBJECT) phrases = pick(1)
  if (`${label}${phrases.join('; ')}`.length > SUBJECT) phrases = pick(0)
  while (phrases.length > 1 && `${label}${phrases.join('; ')}`.length > SUBJECT) phrases = phrases.slice(0, -1)

  const reach = [...callers].filter(([, count]) => count > 0).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
  const callerText = reach.map(([at, count]) => `${count} ${count === 1 ? 'caller' : 'callers'} in ${partLabel(at)}`)
  const withCallers = callerText.length ? [...phrases, callerText[0]] : phrases
  const subject = `${label}${(`${label}${withCallers.join('; ')}`.length <= SUBJECT ? withCallers : phrases).join('; ')}`

  const lines: string[] = []
  for (const file of files) {
    const what = named(file.functions).length
      ? functionPhrases(file.functions, 4).join(', ')
      : ({ added: 'new', modified: 'changed', deleted: 'removed', renamed: `renamed from ${file.oldPath}` })[file.status]
    lines.push(...wrap(`- ${file.path}: ${what}`))
  }
  if (callerText.length) lines.push('', ...wrap(`Called from outside the change: ${callerText.join(', ')}.`))
  return { subject, body: lines.join('\n') }
}
