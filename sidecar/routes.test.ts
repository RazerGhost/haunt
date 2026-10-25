import assert from 'node:assert/strict'
import { test } from 'node:test'
import ts from 'typescript'
import { fileFunctions } from './functions.ts'
import { findRoutes } from './routes.ts'
import type { FunctionNode } from '../shared/types.ts'

/** A program over in-memory files, parsed the way the scanner parses them, with each file's functions. */
function scanned(files: Record<string, string>) {
  const parsed = new Map(Object.entries(files).map(([name, text]) => [name, ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true)]))
  const options: ts.CompilerOptions = { noEmit: true, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, allowImportingTsExtensions: true, types: [], noLib: true }
  const host = ts.createCompilerHost(options)
  host.getSourceFile = name => parsed.get(name)
  host.fileExists = name => parsed.has(name)
  host.readFile = name => files[name]
  host.directoryExists = () => true
  const program = ts.createProgram({ rootNames: [...parsed.keys()], options, host })
  const functions = new Map<string, Map<ts.Node, FunctionNode>>()
  for (const [name, sf] of parsed) functions.set(name, fileFunctions(sf, name.slice(1), false).byNode)
  return { program, functions }
}

test('finds Hono and Express routes with their prefixes, handlers and middleware', () => {
  const { program, functions } = scanned({
    '/controllers.ts': `
      export function store(c: unknown) { return save(c) }
      export function show(c: unknown) { return 1 }
      function save(c: unknown) { return c }
    `,
    '/routes/entries.ts': `
      import { store, show } from '../controllers.ts'
      export const entries = new Hono()
      entries.get('/', show)
      entries.post('/:id', auth, (c: unknown) => store(c))
    `,
    '/app.ts': `
      import { entries } from './routes/entries.ts'
      const app = new Hono().basePath('/api')
      app.route('/entries', entries)
      app.get('/health', () => 'ok')
      const cached = cache.get('/x', { fallback: 1 })
      const router = express.Router()
      router.delete('/items/:id', (req: unknown, res: unknown) => {})
      server.use('/v1', router)
    `
  })
  const routes = findRoutes(program, functions, name => name.slice(1))
  const seen = routes.map(route => [route.method, route.path, route.handlerName, route.middleware, route.file])
  assert.deepEqual(seen, [
    ['GET', '/api/entries', 'show', [], 'routes/entries.ts'],
    ['POST', '/api/entries/:id', '(inline)', ['auth'], 'routes/entries.ts'],
    ['GET', '/api/health', '(inline)', [], 'app.ts'],
    ['DELETE', '/v1/items/:id', '(inline)', [], 'app.ts']
  ])
  assert.equal(routes[0]!.handler, 'controllers.ts:3:7', 'a named handler is its function')
  assert.deepEqual(routes[1]!.calls, ['controllers.ts:2:7'], 'an inline handler lists what it calls')
})

test('follows chains, a router made with a prefix, groups and on()', () => {
  const { program, functions } = scanned({
    '/api.ts': `
      function login() {}
      function me() {}
      function list() {}
      function show() {}
      const api = routes(app, { prefix: '/api', middleware: [] })
      api.post('/auth/login', login)
        .get('/auth/me', me)
        .on(['GET', 'POST'], '/auth/*', session, me)
      api.group({ prefix: '/media' }, group => group
        .get('', list)
        .get('/:id', show))
      cy.on('tap', 'node', (event: unknown) => show(event))
    `
  })
  const routes = findRoutes(program, functions, name => name.slice(1))
  assert.deepEqual(routes.map(route => [route.method, route.path, route.handlerName, route.middleware, route.line]), [
    ['GET, POST', '/api/auth/*', 'me', ['session'], 9],
    ['POST', '/api/auth/login', 'login', [], 7],
    ['GET', '/api/auth/me', 'me', [], 8],
    ['GET', '/api/media', 'list', [], 11],
    ['GET', '/api/media/:id', 'show', [], 12]
  ])
})
