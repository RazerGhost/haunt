/*
 * The languages Haunt reads, by file extension. The TypeScript family goes through the TypeScript compiler (with the
 * type checker for calls): TypeScript, JavaScript, and the script blocks of Vue, Svelte and Astro files (frameworks.ts).
 * The others the app reads with tree-sitter (src-tauri/src/code), with calls matched by name; its copy of this table
 * (code/languages.rs) is tested against the same cases (shared/languages.cases.json).
 */

export type Language = 'typescript' | 'php' | 'rust' | 'lua' | 'python' | 'go'

const TYPESCRIPT = /\.([cm]?tsx?|[cm]?jsx?|vue|svelte|astro)$/
/** Files whose code sits in script blocks, read through a TypeScript view of them (frameworks.ts). */
export const FRAMEWORK = /\.(vue|svelte|astro)$/
/** The TypeScript files proper, which a tsconfig lists. */
export const TYPESCRIPT_PROPER = /\.[cm]?tsx?$/
const DECLARATION = /\.d\.[cm]?ts$/
/** Built or bundled JavaScript: not anyone's source. */
const BUILT = /\.(min|bundle|umd|prod|production)\.[cm]?js$|[.-][0-9a-f]{8,}\.[cm]?js$/

const OTHERS: Record<Exclude<Language, 'typescript'>, RegExp> = {
  php: /\.php$/,
  rust: /\.rs$/,
  lua: /\.lua$/,
  python: /\.py$/,
  go: /\.go$/
}

/** Larger files are generated or vendored, not written. */
export const MAX_SOURCE_BYTES = 512 * 1024

export function languageOf(path: string): Language | null {
  if (DECLARATION.test(path) || BUILT.test(path)) return null
  if (TYPESCRIPT.test(path)) return 'typescript'
  for (const [language, pattern] of Object.entries(OTHERS)) if (pattern.test(path)) return language as Language
  return null
}

export function isSource(path: string): boolean {
  return languageOf(path) !== null
}

/** The tests by their usual names and folders (test helpers in a `testing/` folder too), in every language Haunt reads. */
export function isTestPath(path: string): boolean {
  return /\.(test|spec)\.[cm]?[jt]sx?$/.test(path)
    || /(Test|_test)\.php$/.test(path)
    || /_spec\.lua$|_test\.lua$/.test(path)
    || /(^|\/)test_[^/]*\.py$|_test\.py$|(^|\/)conftest\.py$/.test(path)
    || /_test\.go$/.test(path)
    || /(^|\/)(tests?|testing|spec|__tests__|__mocks__|__fixtures__|fixtures|e2e)\//.test(path)
}
