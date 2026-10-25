/*
 * Vue, Svelte and Astro files as TypeScript: everything outside their script blocks (Astro: the `---` frontmatter and its
 * `<script>` tags) becomes spaces, and every line break stays. The result is valid TypeScript in which every function,
 * import and call sits at the same line and column as in the original file, so the TypeScript compiler, the function
 * tree and the call graph read it as they read any other file. The templates are markup, not code, and are left out.
 */

import ts from 'typescript'
import { FRAMEWORK } from './languages.ts'

/** The TypeScript view of a framework file: its text, and whether its scripts are TSX (`lang="tsx"` or `"jsx"`). */
export function scriptView(path: string, text: string): { text: string, tsx: boolean } {
  const keep: [number, number][] = []
  let tsx = false
  if (path.endsWith('.astro')) {
    const front = /^(\s*---[^\S\r\n]*\r?\n)([\s\S]*?)\r?\n---/.exec(text)
    if (front) keep.push([front[1]!.length, front[1]!.length + front[2]!.length])
  }
  for (const match of text.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi)) {
    const attributes = match[1] ?? ''
    // A script that loads another file has no code of its own here.
    if (/\bsrc\s*=/.test(attributes)) continue
    if (/\blang\s*=\s*["']?(tsx|jsx)\b/.test(attributes)) tsx = true
    const start = match.index! + match[0].indexOf('>') + 1
    keep.push([start, start + (match[2] ?? '').length])
  }
  keep.sort((a, b) => a[0] - b[0])
  let out = ''
  let at = 0
  const blank = (from: number, to: number) => {
    for (let i = from; i < to; i++) {
      const character = text[i]!
      out += character === '\n' || character === '\r' ? character : ' '
    }
  }
  for (const [start, end] of keep) {
    if (start < at) continue
    blank(at, start)
    out += text.slice(start, end)
    at = end
  }
  blank(at, text.length)
  return { text: out, tsx }
}

/** The name the TypeScript compiler knows a framework file by: `Card.svelte` becomes `Card.svelte.ts`. */
export function viewName(file: string, tsx = false): string {
  return `${file}${tsx ? '.tsx' : '.ts'}`
}

/**
 * A file's text as the TypeScript compiler reads it: a framework file as its view, under the view's name; any other by
 * its own name, its kind following the extension (.ts, .tsx, .mts, .d.ts, .js, .jsx, .mjs, .cjs).
 */
export function sourceFileOf(file: string, text: string): ts.SourceFile {
  if (!FRAMEWORK.test(file)) return ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true)
  const view = scriptView(file, text)
  return ts.createSourceFile(viewName(file, view.tsx), view.text, ts.ScriptTarget.Latest, true)
}

/** The framework file behind a view name, or null when the name is not a view. */
export function viewSource(name: string): string | null {
  const match = /^(.*\.(?:vue|svelte|astro))\.tsx?$/.exec(name)
  return match ? match[1]! : null
}
