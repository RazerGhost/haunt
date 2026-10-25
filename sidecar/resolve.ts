import ts from 'typescript'
import type { FunctionNode } from '../shared/types.ts'

/*
 * Following a name to the scan's function it means, with the type checker: through imports and re-exports, a variable
 * or property holding the function, an object shorthand, a default export, and a destructured property
 * (`const { select } = useCode()`). The call graph (calls.ts) and the routes (routes.ts) both resolve this way.
 */

/** The expression inside parentheses, `as`, `satisfies` and `!`. */
export function unwrap(node: ts.Node): ts.Node {
  let inner = node
  while (ts.isParenthesizedExpression(inner) || ts.isAsExpression(inner) || ts.isSatisfiesExpression(inner) || ts.isNonNullExpression(inner)) inner = inner.expression
  return inner
}

export interface Resolver {
  /** The function a declaration is, or holds. */
  declared: (declaration: ts.Declaration, depth?: number) => FunctionNode | undefined
  /** The function a callee (`run`, `service.run`) leads to. */
  target: (callee: ts.Expression) => FunctionNode | undefined
}

export function functionResolver(checker: ts.TypeChecker, byNode: Map<ts.Node, FunctionNode>): Resolver {
  const declared = (declaration: ts.Declaration, depth = 0): FunctionNode | undefined => {
    const direct = byNode.get(declaration)
    if (direct) return direct
    if ((ts.isVariableDeclaration(declaration) || ts.isPropertyAssignment(declaration) || ts.isPropertyDeclaration(declaration)) && declaration.initializer) {
      return byNode.get(unwrap(declaration.initializer))
    }
    if (ts.isShorthandPropertyAssignment(declaration) && depth < 3) {
      const value = checker.getShorthandAssignmentValueSymbol(declaration)
      for (const inner of value?.declarations ?? []) {
        const found = declared(inner, depth + 1)
        if (found) return found
      }
    }
    if (ts.isExportAssignment(declaration)) return byNode.get(unwrap(declaration.expression))
    // `const { select } = useCode()`: the property it takes, of what it destructures.
    if (ts.isBindingElement(declaration) && ts.isObjectBindingPattern(declaration.parent) && depth < 3) {
      const key = declaration.propertyName ?? declaration.name
      const name = ts.isIdentifier(key) || ts.isStringLiteral(key) ? key.text : undefined
      const property = name ? checker.getTypeAtLocation(declaration.parent).getProperty(name) : undefined
      for (const inner of property?.declarations ?? []) {
        const found = declared(inner, depth + 1)
        if (found) return found
      }
    }
    return undefined
  }

  const target = (callee: ts.Expression): FunctionNode | undefined => {
    const inner = unwrap(callee)
    const at = ts.isPropertyAccessExpression(inner) ? inner.name : ts.isIdentifier(inner) ? inner : undefined
    if (!at) return undefined
    let symbol = checker.getSymbolAtLocation(at)
    if (!symbol) return undefined
    if (symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol)
    for (const declaration of symbol.declarations ?? []) {
      const found = declared(declaration)
      if (found) return found
    }
    return undefined
  }

  return { declared, target }
}
