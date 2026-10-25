import ts from 'typescript'
import type { FunctionKind, FunctionNode } from '../shared/types.ts'

/*
 * The functions a file defines, as a tree: a function written inside another is its child. Unnamed callbacks
 * (`items.map(item => …)`) only appear when a named function sits inside them; otherwise every arrow would be a node.
 * In a test file, a test's callback (`it('adds', () => …)`, `test.each(rows)('…', …)`, node's `t.test('…', …)`) is
 * kept, named by its title and marked `test`, so its calls count and the tests that reach a change can be found.
 */

export interface FileFunctions {
  tree: FunctionNode[]
  /** Every node of the tree by its declaration, for the call graph. */
  byNode: Map<ts.Node, FunctionNode>
}

type FunctionLike = ts.FunctionDeclaration | ts.FunctionExpression | ts.ArrowFunction | ts.MethodDeclaration
  | ts.ConstructorDeclaration | ts.GetAccessorDeclaration | ts.SetAccessorDeclaration | ts.ClassDeclaration | ts.ClassExpression

export function isFunctionLike(node: ts.Node): node is FunctionLike {
  return ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node) || ts.isArrowFunction(node)
    || ts.isMethodDeclaration(node) || ts.isConstructorDeclaration(node) || ts.isGetAccessor(node) || ts.isSetAccessor(node)
    || ts.isClassDeclaration(node) || ts.isClassExpression(node)
}

function kindOf(node: FunctionLike): FunctionKind {
  if (ts.isClassDeclaration(node) || ts.isClassExpression(node)) return 'class'
  if (ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node)) return 'function'
  if (ts.isArrowFunction(node)) return 'arrow'
  return 'method'
}

/** The node a function is bound to: `const x = () => …`, `{ x: () => … }`, `x = () => …` in a class. */
function holder(node: ts.Node): ts.Node | undefined {
  let parent = node.parent
  while (parent && (ts.isParenthesizedExpression(parent) || ts.isAsExpression(parent) || ts.isSatisfiesExpression(parent))) parent = parent.parent
  if (parent && (ts.isVariableDeclaration(parent) || ts.isPropertyAssignment(parent) || ts.isPropertyDeclaration(parent) || ts.isExportAssignment(parent))) return parent
  return undefined
}

function propertyName(name: ts.PropertyName | ts.BindingName, sf: ts.SourceFile): string {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isPrivateIdentifier(name) || ts.isNumericLiteral(name)) return name.text
  return name.getText(sf)
}

function nameOf(node: FunctionLike, sf: ts.SourceFile): string | null {
  if (ts.isConstructorDeclaration(node)) return 'constructor'
  if (node.name) return propertyName(node.name, sf)
  const bound = holder(node)
  if (bound && ts.isExportAssignment(bound)) return 'default'
  if (bound && (ts.isVariableDeclaration(bound) || ts.isPropertyAssignment(bound) || ts.isPropertyDeclaration(bound))) return propertyName(bound.name, sf)
  if (ts.isCallExpression(node.parent) || ts.isNewExpression(node.parent)) {
    let callee = node.parent.expression.getText(sf).replace(/\s+/g, '')
    if (callee.length > 40) callee = `…${callee.slice(-39)}`
    return `→ ${callee}()`
  }
  return null
}

/** The calls that register one test, by the first name of their callee (`it.only`, `test.skip`, `t.test` too). */
const TEST_CALLS = new Set(['it', 'test', 'bench'])

/** A test callback's title, when `node` is the callback of a test call with a written title; null otherwise. */
function testTitle(node: FunctionLike, sf: ts.SourceFile): string | null {
  const call = node.parent
  if (!call || !ts.isCallExpression(call) || call.arguments[0] === node || !call.arguments.includes(node as ts.Expression)) return null
  let callee: ts.Expression = call.expression
  // test.each(rows)('…', fn): the title belongs to the outer call, the name to the inner one.
  if (ts.isCallExpression(callee)) callee = callee.expression
  const names: string[] = []
  while (ts.isPropertyAccessExpression(callee)) {
    names.push(callee.name.text)
    callee = callee.expression
  }
  if (ts.isIdentifier(callee)) names.push(callee.text)
  if (!names.some(name => TEST_CALLS.has(name))) return null
  const title = call.arguments[0]!
  if (ts.isStringLiteral(title) || ts.isNoSubstitutionTemplateLiteral(title)) return title.text
  if (ts.isTemplateExpression(title)) return title.getText(sf).slice(1, -1)
  return null
}

function hasModifier(node: ts.Node, kind: ts.SyntaxKind): boolean {
  return ts.canHaveModifiers(node) && !!ts.getModifiers(node)?.some(modifier => modifier.kind === kind)
}

function isExported(node: FunctionLike): boolean {
  if (hasModifier(node, ts.SyntaxKind.ExportKeyword)) return true
  const bound = holder(node)
  if (bound && ts.isExportAssignment(bound)) return true
  // const x = …, inside `export const`
  return !!bound && ts.isVariableDeclaration(bound) && hasModifier(bound.parent.parent, ts.SyntaxKind.ExportKeyword)
}

export function fileFunctions(sf: ts.SourceFile, path: string, testFile = false): FileFunctions {
  const byNode = new Map<ts.Node, FunctionNode>()
  const visit = (node: ts.Node, into: FunctionNode[]): void => {
    if (!isFunctionLike(node)) {
      ts.forEachChild(node, child => visit(child, into))
      return
    }
    const title = testFile ? testTitle(node, sf) : null
    const name = title ?? nameOf(node, sf)
    const anonymous = !title && !node.name && !ts.isConstructorDeclaration(node) && !holder(node)
    const start = sf.getLineAndCharacterOfPosition(node.getStart(sf))
    const item: FunctionNode = {
      id: `${path}:${start.line + 1}:${start.character + 1}`,
      name: name ?? '(anonymous)',
      kind: kindOf(node),
      line: start.line + 1,
      endLine: sf.getLineAndCharacterOfPosition(node.getEnd()).line + 1,
      exported: isExported(node),
      async: hasModifier(node, ts.SyntaxKind.AsyncKeyword),
      anonymous,
      ...title === null ? {} : { test: true },
      children: []
    }
    ts.forEachChild(node, child => visit(child, item.children))
    if (anonymous && !item.children.some(child => !child.anonymous || child.children.length)) {
      // Not kept: its children (unnamed callbacks without named ones inside) are dropped with it.
      return
    }
    byNode.set(node, item)
    into.push(item)
  }
  const tree: FunctionNode[] = []
  visit(sf, tree)
  // Children of a dropped callback were registered before it was dropped; keep only what is in the tree.
  const kept = new Set<FunctionNode>()
  const mark = (items: FunctionNode[]): void => items.forEach(item => { kept.add(item); mark(item.children) })
  mark(tree)
  for (const [node, item] of byNode) if (!kept.has(item)) byNode.delete(node)
  return { tree, byNode }
}
