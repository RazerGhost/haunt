import { join, sep } from 'node:path'
import type { FunctionNode } from '../shared/types.ts'
import { sourceFileOf } from './frameworks.ts'
import { fileFunctions } from './functions.ts'
import { isTestPath, languageOf } from './languages.ts'

/* Another version of a file (its text at a commit), read as the scan reads the file: for the changes by function. */

/** The functions of `text` as the file `path` (relative to `root`); none for a file that is not of the TypeScript family. */
export function versionFunctions(root: string, path: string, text: string): FunctionNode[] {
  if (languageOf(path) !== 'typescript') return []
  const name = join(root, path).split(sep).join('/')
  const sf = sourceFileOf(name, text)
  // As the scan reads it: a test file's test callbacks are functions (named by their titles).
  return fileFunctions(sf, path, isTestPath(path)).tree
}
