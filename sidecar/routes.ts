import ts from 'typescript'
import type { FunctionNode } from '../shared/types.ts'
import { functionResolver } from './resolve.ts'

/*
 * The HTTP routes in the TypeScript family's code, by the framework rules Hono and Express share: `app.get('/path',
 * …middleware, handler)` for get, post, put, patch, delete, options, head and all, Hono's `on(['GET', 'POST'], …)`, and
 * chains of them (`api.get(…).post(…)`). A path is a string; the handler is a function, or a name the type checker
 * follows to one. Mounted routers add their prefix: Hono's `app.route('/api', entries)` and `basePath('/api')`,
 * Express's `app.use('/api', router)`, a router made with `{ prefix: '/api' }`, and a group's callback
 * (`api.group({ prefix: '/media' }, group => …)`), followed across files through the checker. A prefix passed into a
 * function that registers routes is not followed.
 *
 * An inline handler has no function of its own in the scan (an unnamed callback is kept only when named functions sit
 * inside it): then the route lists the functions the handler calls, which is where its call path goes on. The PHP
 * routes (WordPress) are read by the app (src-tauri/src/code/routes.rs).
 */

export interface Route {
  method: string
  path: string
  file: string
  line: number
  framework: 'hono-express' | 'wordpress'
  /** The handler's function id in the scan, when it has one. */
  handler: string | null
  /** The handler as written: a name, or `(inline)`. */
  handlerName: string
  /** The functions an inline handler calls, by id: where its call path goes on. */
  calls: string[]
  /** The middleware in between, by name. */
  middleware: string[]
}

const METHODS = new Set(['get', 'post', 'put', 'patch', 'delete', 'options', 'head', 'all'])
/** The methods Hono's `on()` names. */
const HTTP_METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD', 'ALL'])
/** A route's path: `/…` or `*`. */
const isPath = (value: string | null): value is string => value !== null && (value.startsWith('/') || value === '*')
/** The other calls a chain of routes goes through: `api.get(…).on(…).group(…).use(…)`. */
const CHAINED = new Set(['on', 'use', 'route', 'group', 'basePath', 'middleware'])
/** The most functions an inline handler's call list holds. */
const MOST_CALLS = 8

function unwrap(node: ts.Expression): ts.Expression {
  let inner = node
  while (ts.isParenthesizedExpression(inner) || ts.isAsExpression(inner) || ts.isSatisfiesExpression(inner) || ts.isNonNullExpression(inner) || ts.isAwaitExpression(inner)) inner = inner.expression
  return inner
}

const literal = (node: ts.Expression | undefined): string | null =>
  node && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) ? node.text : null

/** A short name for an argument: `auth`, `zValidator(…)`, `cors()`. */
function nameOf(node: ts.Expression, sf: ts.SourceFile): string {
  const inner = unwrap(node)
  if (ts.isCallExpression(inner)) return `${nameOf(inner.expression, sf)}(…)`
  if (ts.isArrowFunction(inner) || ts.isFunctionExpression(inner)) return '(inline)'
  const text = inner.getText(sf).replace(/\s+/g, ' ')
  return text.length > 40 ? `${text.slice(0, 39)}…` : text
}

export function findRoutes(program: ts.Program, files: Map<string, Map<ts.Node, FunctionNode>>, pathOf: (fileName: string) => string | undefined): Route[] {
  const checker = program.getTypeChecker()
  const byNode = new Map<ts.Node, FunctionNode>()
  for (const nodes of files.values()) for (const [node, item] of nodes) byNode.set(node, item)

  /** The declaration a name or property access refers to, through imports. */
  const declarationOf = (node: ts.Expression): ts.Declaration | undefined => {
    const inner = unwrap(node)
    const at = ts.isPropertyAccessExpression(inner) ? inner.name : ts.isIdentifier(inner) ? inner : undefined
    if (!at) return undefined
    let symbol = checker.getSymbolAtLocation(at)
    if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol)
    return symbol?.declarations?.[0]
  }
  // A handler named in the route resolves as the call graph resolves it (resolve.ts).
  const { target } = functionResolver(checker, byNode)
  /** The scan's function for an expression: the function itself, or what a name refers to. */
  const functionOf = (node: ts.Expression): FunctionNode | undefined => byNode.get(unwrap(node)) ?? target(unwrap(node))
  const isFunctionLike = (node: ts.Expression): boolean => {
    const inner = unwrap(node)
    if (ts.isArrowFunction(inner) || ts.isFunctionExpression(inner)) return true
    if (ts.isIdentifier(inner) || ts.isPropertyAccessExpression(inner) || ts.isCallExpression(inner)) {
      return checker.getTypeAtLocation(inner).getCallSignatures().length > 0
    }
    return false
  }
  /** The functions called inside an inline handler, by id, in order. */
  const callsIn = (node: ts.Node): string[] => {
    const out: string[] = []
    const visit = (child: ts.Node): void => {
      if (out.length >= MOST_CALLS) return
      if (ts.isCallExpression(child)) {
        const called = functionOf(child.expression)
        if (called && !called.anonymous && !out.includes(called.id)) out.push(called.id)
      }
      ts.forEachChild(child, visit)
    }
    ts.forEachChild(node, visit)
    return out
  }

  // The routers' own prefixes and where they are mounted, by the declaration of the variable that holds them.
  const basePath = new Map<ts.Declaration, string>()
  const mounts = new Map<ts.Declaration, { parent: ts.Declaration | undefined, prefix: string }>()
  /** `pathAt`: the path's argument (Hono's `on()` has the methods first). */
  const found: { node: ts.CallExpression, receiver: ts.Declaration | undefined, method: string, path: string, sf: ts.SourceFile, pathAt?: number }[] = []
  /** `{ prefix: '/api' }`'s prefix. */
  const prefixOption = (argument: ts.Expression): string | null => {
    const inner = unwrap(argument)
    if (!ts.isObjectLiteralExpression(inner)) return null
    for (const property of inner.properties) {
      if (ts.isPropertyAssignment(property) && ts.isIdentifier(property.name) && property.name.text === 'prefix') return literal(unwrap(property.initializer))
    }
    return null
  }
  /** The router a chain of route calls starts from: `api.get(…).post(…)` is `api`. */
  const rootOf = (expression: ts.Expression): ts.Expression => {
    let inner = unwrap(expression)
    while (ts.isCallExpression(inner) && ts.isPropertyAccessExpression(inner.expression) && (METHODS.has(inner.expression.name.text) || CHAINED.has(inner.expression.name.text))) {
      inner = unwrap(inner.expression.expression)
    }
    return inner
  }

  for (const fileName of files.keys()) {
    const sf = program.getSourceFile(fileName)
    if (!sf) continue
    const visit = (node: ts.Node): void => {
      if (ts.isVariableDeclaration(node) && node.initializer) {
        // `const api = new Hono().basePath('/api')`, or a router made with a prefix: `routes(app, { prefix: '/api' })`.
        let chain: ts.Expression = unwrap(node.initializer)
        while (ts.isCallExpression(chain)) {
          const prefix = chain.arguments.map(prefixOption).find(found => found !== null) ?? null
          if (prefix !== null) basePath.set(node, prefix)
          if (!ts.isPropertyAccessExpression(chain.expression)) break
          if (chain.expression.name.text === 'basePath') {
            const base = literal(chain.arguments[0])
            if (base !== null) basePath.set(node, base)
          }
          chain = unwrap(chain.expression.expression)
        }
      }
      if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)) {
        const method = node.expression.name.text
        // Asked of the type checker only for a call that turns out to be a route or a mount, not for every method call.
        const receiver = () => declarationOf(rootOf((node.expression as ts.PropertyAccessExpression).expression))
        const first = literal(node.arguments[0])
        const last = node.arguments[node.arguments.length - 1]
        if ((method === 'route' || method === 'use') && first !== null && node.arguments.length === 2) {
          // A router mounted under a prefix: Hono's route(), Express's use().
          const mounted = unwrap(node.arguments[1]!)
          const child = ts.isArrowFunction(mounted) || ts.isFunctionExpression(mounted) ? undefined : declarationOf(mounted)
          if (child) mounts.set(child, { parent: receiver(), prefix: first })
        } else if (method === 'group' && last && (ts.isArrowFunction(unwrap(last)) || ts.isFunctionExpression(unwrap(last)))) {
          // `api.group({ prefix: '/media' }, group => group.get(…))`: the callback's parameter is a router under it.
          const prefix = node.arguments.map(prefixOption).find(found => found !== null) ?? literal(node.arguments[0]) ?? ''
          const parameter = (unwrap(last) as ts.ArrowFunction).parameters[0]
          if (parameter) mounts.set(parameter, { parent: receiver(), prefix })
        } else if (METHODS.has(method) && first !== null && (first.startsWith('/') || first === '*' || first === '') && node.arguments.length >= 2 && isFunctionLike(last!)) {
          found.push({ node, receiver: receiver(), method, path: first, sf })
        } else if (method === 'on' && node.arguments.length >= 3 && isPath(literal(node.arguments[1])) && isFunctionLike(last!)) {
          // Hono's `app.on(['GET', 'POST'], '/path', handler)`; not an event listener's `on('tap', 'node', …)`.
          const listed = unwrap(node.arguments[0]!)
          const methods = ts.isArrayLiteralExpression(listed) ? listed.elements.map(element => literal(element)).filter(item => item !== null) : [literal(listed)].filter(item => item !== null)
          if (methods.length && methods.every(item => HTTP_METHODS.has(item.toUpperCase()))) found.push({ node, receiver: receiver(), method: methods.join(', '), path: literal(node.arguments[1])!, sf, pathAt: 1 })
        }
      }
      ts.forEachChild(node, visit)
    }
    visit(sf)
  }

  const prefixOf = (declaration: ts.Declaration | undefined, depth = 0): string => {
    if (!declaration || depth > 8) return ''
    const own = basePath.get(declaration) ?? ''
    const mount = mounts.get(declaration)
    return mount ? `${prefixOf(mount.parent, depth + 1)}${mount.prefix}${own}` : own
  }
  const join = (prefix: string, path: string) => {
    const joined = `${prefix.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`
    return joined.length > 1 ? joined.replace(/\/+$/, '') : joined
  }

  const routes: Route[] = []
  for (const { node, receiver, method, path, sf, pathAt = 0 } of found) {
    const file = pathOf(sf.fileName)
    if (!file) continue
    const handlerNode = node.arguments[node.arguments.length - 1]!
    const handler = functionOf(handlerNode)
    const inline = ts.isArrowFunction(unwrap(handlerNode)) || ts.isFunctionExpression(unwrap(handlerNode))
    // In a chain the call starts where the chain does: the method's name is where this route is written.
    const at = ts.isPropertyAccessExpression(node.expression) ? node.expression.name : node
    routes.push({
      method: method === 'all' ? 'ALL' : method.toUpperCase(),
      path: join(prefixOf(receiver), path),
      file,
      line: sf.getLineAndCharacterOfPosition(at.getStart(sf)).line + 1,
      framework: 'hono-express',
      handler: handler && !handler.anonymous ? handler.id : null,
      handlerName: handler && !handler.anonymous ? handler.name : nameOf(handlerNode, sf),
      calls: inline && !(handler && !handler.anonymous) ? callsIn(unwrap(handlerNode)) : [],
      middleware: node.arguments.slice(pathAt + 1, -1).map(argument => nameOf(argument, sf))
    })
  }
  return routes.sort((a, b) => a.path.localeCompare(b.path) || a.method.localeCompare(b.method))
}
