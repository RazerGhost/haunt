import ts from 'typescript'
import type { CallEdge, FunctionNode } from '../shared/types.ts'
import { functionResolver, unwrap } from './resolve.ts'

/*
 * Who calls whom, resolved by the type checker: `run()`, `service.run()`, `this.run()` and `new Runner()` each lead to
 * the declaration they reach, across files and through imports and re-exports. A call through an interface (a port,
 * a context member typed by an interface) stops at the interface and is left out; so are calls into packages that
 * were not scanned. A call inside an unnamed callback counts for the function the callback is written in.
 *
 * The same walk notes every use of a function, called or not: passed as a value (`items.map(format)`), put in an object,
 * re-exported under another name, or reached through a global that only says it is that function's type (Nuxt's
 * auto-imports, `const useX: typeof import('…')['useX']`). A use from another file is a use from outside; a function's
 * mention of itself is not a use. What nothing uses is likely dead; an export used only in its own file need not be one.
 */

/**
 * How a function is used: from another file, or only from its own; `held` when nothing names it but it is written into
 * an object (`{ onSelect: () => … }`, a computed's `get`), which whoever is handed the object calls; absent when nothing
 * uses it.
 */
export type Uses = Record<string, 'outside' | 'inside' | 'held'>

export interface CallGraph { calls: CallEdge[], uses: Uses }

/** The calls between functions, and how each function is used. */
export function callGraph(program: ts.Program, files: Map<string, Map<ts.Node, FunctionNode>>): CallGraph {
  const steps = callGraphSteps(program, files)
  for (;;) {
    const step = steps.next()
    if (step.done) return step.value
  }
}

/**
 * The same, worked out in turns: it gives way to whatever else is waiting (a question from the app) about every 20
 * milliseconds, and stops (null) once `stop` says a newer scan wants the time instead.
 */
export async function callGraphInTurns(program: ts.Program, files: Map<string, Map<ts.Node, FunctionNode>>, stop: () => boolean): Promise<CallGraph | null> {
  const steps = callGraphSteps(program, files)
  let since = performance.now()
  for (;;) {
    const step = steps.next()
    if (step.done) return step.value
    if (performance.now() - since < 20) continue
    await new Promise(resolve => setImmediate(resolve))
    if (stop()) return null
    since = performance.now()
  }
}

/** The walk, one file at a time: it yields after each file. */
function* callGraphSteps(program: ts.Program, files: Map<string, Map<ts.Node, FunctionNode>>): Generator<void, CallGraph, void> {
  const checker = program.getTypeChecker()
  const byNode = new Map<ts.Node, FunctionNode>()
  for (const nodes of files.values()) for (const [node, item] of nodes) byNode.set(node, item)

  const { declared, target } = functionResolver(checker, byNode)

  /** The function a name refers to, through aliases, and through a global typed as that function. */
  const referred = (name: ts.Identifier): FunctionNode | undefined => {
    let symbol = checker.getSymbolAtLocation(name)
    if (!symbol) return undefined
    if (symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol)
    for (const declaration of symbol.declarations ?? []) {
      const found = declared(declaration)
      if (found) return found
    }
    const ambient = symbol.valueDeclaration
    if (ambient && ambient.getSourceFile().isDeclarationFile && ts.isVariableDeclaration(ambient)) {
      for (const declaration of checker.getTypeOfSymbolAtLocation(symbol, name).getSymbol()?.declarations ?? []) {
        const found = declared(declaration)
        if (found) return found
      }
    }
    return undefined
  }
  /** Not a use: the name a declaration gives, or the names an import statement lists. */
  const naming = (name: ts.Identifier): boolean => {
    const parent = name.parent
    if (ts.isImportSpecifier(parent) || ts.isImportClause(parent) || ts.isNamespaceImport(parent)) return true
    return 'name' in parent && (parent as ts.NamedDeclaration).name === name && !ts.isPropertyAccessExpression(parent)
  }
  /** What a name in an export list (`export { a }`, `export { a } from './x'`) or an object shorthand (`{ a }`) uses. */
  const listed = (name: ts.Identifier): FunctionNode | undefined => {
    const parent = name.parent
    let symbol: ts.Symbol | undefined
    if (ts.isExportSpecifier(parent) && (parent.propertyName ?? parent.name) === name) symbol = checker.getExportSpecifierLocalTargetSymbol(parent)
    else if (ts.isShorthandPropertyAssignment(parent) && parent.name === name) symbol = checker.getShorthandAssignmentValueSymbol(parent)
    else return undefined
    if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol)
    for (const declaration of symbol?.declarations ?? []) {
      const found = declared(declaration)
      if (found) return found
    }
    return undefined
  }
  const fileOf = new Map<FunctionNode, string>()
  for (const [path, nodes] of files) for (const item of nodes.values()) fileOf.set(item, path)
  const uses: Uses = {}

  const counts = new Map<string, number>()
  for (const path of files.keys()) {
    const sf = program.getSourceFile(path)
    if (!sf) continue
    const visit = (node: ts.Node, caller: FunctionNode | undefined): void => {
      const own = byNode.get(node) ?? caller
      if (ts.isIdentifier(node) && (ts.isExportSpecifier(node.parent) || ts.isShorthandPropertyAssignment(node.parent))) {
        // Exported on from another file (a package's face), or put in an object: used. Listing a file's own function in
        // its own export list is not a use.
        const used = listed(node)
        if (used && used !== own) {
          if (fileOf.get(used) !== path) uses[used.id] = 'outside'
          else if (ts.isShorthandPropertyAssignment(node.parent)) uses[used.id] ??= 'inside'
        }
      } else if (ts.isIdentifier(node) && !naming(node)) {
        const used = referred(node)
        if (used && used !== own) {
          if (fileOf.get(used) !== path) uses[used.id] = 'outside'
          else uses[used.id] ??= 'inside'
        }
      }
      // `export * from './x'`: everything x exports is part of this file's face, so used from outside x.
      if (ts.isExportDeclaration(node) && !node.exportClause && node.moduleSpecifier) {
        const module = checker.getSymbolAtLocation(node.moduleSpecifier)
        for (let symbol of module ? checker.getExportsOfModule(module) : []) {
          if (symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol)
          for (const declaration of symbol.declarations ?? []) {
            const found = declared(declaration)
            if (found && fileOf.get(found) !== path) uses[found.id] = 'outside'
          }
        }
      }
      if ((ts.isCallExpression(node) || ts.isNewExpression(node)) && own) {
        const callee = target(node.expression)
        if (callee && callee !== own) {
          const key = `${own.id}\n${callee.id}`
          counts.set(key, (counts.get(key) ?? 0) + 1)
        }
      }
      ts.forEachChild(node, child => visit(child, own))
    }
    visit(sf, undefined)
    yield
  }
  // Written into an object literal, or a function given as a value where it is written (returned, passed, assigned):
  // used by whoever gets it, which no name shows.
  for (const [node, item] of byNode) {
    if (uses[item.id]) continue
    let parent = node.parent
    while (parent && (ts.isParenthesizedExpression(parent) || ts.isAsExpression(parent) || ts.isSatisfiesExpression(parent))) parent = parent.parent
    const expression = ts.isFunctionExpression(node) || ts.isArrowFunction(node) || ts.isClassExpression(node)
    const held = (parent && ts.isPropertyAssignment(parent) && ts.isObjectLiteralExpression(parent.parent))
      || ((ts.isMethodDeclaration(node) || ts.isGetAccessor(node) || ts.isSetAccessor(node)) && ts.isObjectLiteralExpression(node.parent))
      || (expression && !!parent && !ts.isVariableDeclaration(parent) && !ts.isPropertyDeclaration(parent))
    if (held) uses[item.id] = 'held'
  }
  const calls = [...counts].map(([key, count]) => {
    const [from, to] = key.split('\n') as [string, string]
    return { from, to, count }
  })
  return { calls, uses }
}
