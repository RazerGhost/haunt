import { sep } from 'node:path'
import type ts from 'typescript'
import type { FileImport, InnerImport, PartImport, Rule } from '../shared/types.ts'

/*
 * The import graph of the scanned files: every `import`, `export … from`, `import()` and `import x = require()` with a
 * literal, resolved the way TypeScript resolves it (the project's options: path aliases, package exports, workspace
 * packages through their links), kept when it lands on another scanned file. Type-only imports are marked: they couple
 * parts, but are gone at run time, so they never make a cycle.
 */

export interface FileEdge {
  from: string
  to: string
  typeOnly: boolean
}

/** The module specifiers a file imports, and whether only types come through. */
export function importsIn(sf: ts.SourceFile, typescript: typeof ts): { specifier: string, typeOnly: boolean }[] {
  const found: { specifier: string, typeOnly: boolean }[] = []
  const visit = (node: ts.Node): void => {
    if (typescript.isImportDeclaration(node) && typescript.isStringLiteral(node.moduleSpecifier)) {
      const clause = node.importClause
      const named = clause?.namedBindings && typescript.isNamedImports(clause.namedBindings) ? clause.namedBindings.elements : null
      // `import type …`, or only `{ type A, type B }`; a side-effect import (no clause) runs the module.
      const typeOnly = !!clause && (clause.isTypeOnly || (!clause.name && !!named && named.length > 0 && named.every(element => element.isTypeOnly)))
      found.push({ specifier: node.moduleSpecifier.text, typeOnly })
    } else if (typescript.isExportDeclaration(node) && node.moduleSpecifier && typescript.isStringLiteral(node.moduleSpecifier)) {
      const named = node.exportClause && typescript.isNamedExports(node.exportClause) ? node.exportClause.elements : null
      found.push({ specifier: node.moduleSpecifier.text, typeOnly: node.isTypeOnly || (!!named && named.length > 0 && named.every(element => element.isTypeOnly)) })
    } else if (typescript.isCallExpression(node) && node.expression.kind === typescript.SyntaxKind.ImportKeyword && node.arguments[0] && typescript.isStringLiteralLike(node.arguments[0])) {
      found.push({ specifier: node.arguments[0].text, typeOnly: false })
    } else if (typescript.isImportEqualsDeclaration(node) && typescript.isExternalModuleReference(node.moduleReference) && typescript.isStringLiteral(node.moduleReference.expression)) {
      found.push({ specifier: node.moduleReference.expression.text, typeOnly: node.isTypeOnly })
    }
    typescript.forEachChild(node, visit)
  }
  visit(sf)
  return found
}

/**
 * The edges between scanned files. `sources` are the parsed files by repository path; a file is known by the name its
 * source file has (a Vue, Svelte or Astro file by its TypeScript view's name). `host` resolves like the scanner's
 * program does, views included (`typescript.sys` when there are none).
 */
export function importGraph(root: string, sources: Map<string, ts.SourceFile>, options: ts.CompilerOptions, typescript: typeof ts, host: ts.ModuleResolutionHost = typescript.sys): FileEdge[] {
  const toPath = new Map<string, string>()
  for (const [path, sf] of sources) toPath.set(pathKey(sf.fileName), path)
  const cache = typescript.createModuleResolutionCache(root, name => typescript.sys.useCaseSensitiveFileNames ? name : name.toLowerCase(), options)
  const edges = new Map<string, FileEdge>()
  for (const [from, sf] of sources) {
    for (const { specifier, typeOnly } of importsIn(sf, typescript)) {
      const resolved = typescript.resolveModuleName(specifier, sf.fileName, options, host, cache).resolvedModule
      if (!resolved) continue
      const to = toPath.get(pathKey(resolved.resolvedFileName))
      if (!to || to === from) continue
      const key = `${from}\n${to}`
      const known = edges.get(key)
      // A file imported for its values somewhere is a runtime import, however often it is imported for types.
      if (known) known.typeOnly &&= typeOnly
      else edges.set(key, { from, to, typeOnly })
    }
  }
  return [...edges.values()]
}

/** A file name to look files up by: forward slashes, and lowercased where the file system ignores case. */
export function pathKey(file: string): string {
  const forward = file.split(sep).join('/')
  return process.platform === 'win32' || process.platform === 'darwin' ? forward.toLowerCase() : forward
}

/** The runtime import cycles (strongly connected components of two or more files), each sorted, the list sorted. */
export function runtimeCycles(files: string[], edges: FileEdge[]): string[][] {
  const next = new Map<string, string[]>()
  for (const file of files) next.set(file, [])
  for (const edge of edges) if (!edge.typeOnly) next.get(edge.from)?.push(edge.to)
  // Tarjan's algorithm, iterative so a deep graph cannot overflow the stack.
  const index = new Map<string, number>(), low = new Map<string, number>(), onStack = new Set<string>()
  const stack: string[] = [], cycles: string[][] = []
  let counter = 0
  const enter = (node: string) => {
    index.set(node, counter)
    low.set(node, counter)
    counter++
    stack.push(node)
    onStack.add(node)
  }
  for (const start of files) {
    if (index.has(start)) continue
    const work: { node: string, edge: number }[] = [{ node: start, edge: 0 }]
    enter(start)
    while (work.length) {
      const frame = work[work.length - 1]!
      const out = next.get(frame.node) ?? []
      if (frame.edge < out.length) {
        const to = out[frame.edge++]!
        if (!index.has(to)) {
          enter(to)
          work.push({ node: to, edge: 0 })
        } else if (onStack.has(to)) {
          low.set(frame.node, Math.min(low.get(frame.node)!, index.get(to)!))
        }
        continue
      }
      work.pop()
      const parent = work[work.length - 1]
      if (parent) low.set(parent.node, Math.min(low.get(parent.node)!, low.get(frame.node)!))
      if (low.get(frame.node) === index.get(frame.node)) {
        const component: string[] = []
        let member: string
        do {
          member = stack.pop()!
          onStack.delete(member)
          component.push(member)
        } while (member !== frame.node)
        if (component.length > 1) cycles.push(component.sort())
      }
    }
  }
  return cycles.sort((a, b) => a[0]!.localeCompare(b[0]!))
}

/** What an import between two parts breaks, if anything. */
export type Judge = (from: { part: string, path: string }, to: { part: string, path: string }) => Rule[]

/** The file edges within one part: what its own folders import from each other. */
export function innerImports(edges: FileEdge[], partOf: (path: string) => string): InnerImport[] {
  return edges.filter(edge => partOf(edge.from) === partOf(edge.to)).map(edge => ({ from: edge.from, to: edge.to, typeOnly: edge.typeOnly }))
}

/** The file edges between different parts, grouped by the pair of parts. */
export function partImports(edges: FileEdge[], partOf: (path: string) => string, judge: Judge | null): PartImport[] {
  const groups = new Map<string, PartImport>()
  for (const edge of edges) {
    const from = partOf(edge.from), to = partOf(edge.to)
    if (from === to) continue
    const file: FileImport = { from: edge.from, to: edge.to, typeOnly: edge.typeOnly, breaks: judge ? judge({ part: from, path: edge.from }, { part: to, path: edge.to }) : [], known: false }
    const key = `${from}\n${to}`
    const group = groups.get(key) ?? { from, to, files: [] }
    group.files.push(file)
    groups.set(key, group)
  }
  return [...groups.values()]
}
