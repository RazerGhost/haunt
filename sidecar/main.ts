import * as nodeModule from 'node:module'
import { pathToFileURL } from 'node:url'
import { ownTypescript } from './compiler.ts'

/*
 * The sidecar: `node sidecar/main.ts <repository>`, started by the app (src-tauri/src/sidecar.rs) with HAUNT_TRUSTED=1
 * when you trust the repository's folder. It talks over stdin and stdout, one JSON message per line (serve.ts).
 * Loading a package runs its code, so the scan uses the repository's own TypeScript (the version it is checked with,
 * and the one its architecture scripts load) only in a trusted folder; otherwise, and when the repository has none,
 * Haunt's own (the desktop app ships one). Bun and Deno run the sidecar too (src-tauri/src/startup.rs); without Node's
 * `registerHooks` they always use Haunt's own TypeScript. A project's TypeScript without the compiler API the scan calls
 * (TypeScript 7, the native compiler: compiler.ts) is left aside for Haunt's own too, and the app is told once (the
 * event `typescript`, with both versions) so it can say why.
 */

// stdout carries only the protocol: whatever else is printed (by a library, a project's script) goes to stderr.
const protocol = process.stdout.write.bind(process.stdout)
process.stdout.write = process.stderr.write.bind(process.stderr) as typeof process.stdout.write
console.log = console.error
console.info = console.error
;(globalThis as { hauntWrite?: (line: string) => void }).hauntWrite = line => protocol(`${line}\n`)

const root = process.argv[2]
if (!root) throw new Error('Run it from the app: node sidecar/main.ts <repository>')
const trusted = process.env.HAUNT_TRUSTED === '1'

let typescript: string | null = null
if (trusted) {
  // Not installed there, or without the API the scan needs: Haunt's own.
  const own = ownTypescript(root)
  if (own && !own.missing.length) typescript = pathToFileURL(own.path).href
  else if (own) {
    let bundled = 'unknown'
    try {
      bundled = (nodeModule.createRequire(import.meta.url)('typescript') as { version?: string }).version ?? bundled
    } catch { /* none of its own either: the import below says so */ }
    protocol(`${JSON.stringify({ event: 'typescript', version: own.version, bundled, missing: own.missing })}\n`)
  }
}

// Named exports a runtime lacks fail the whole import, so registerHooks is looked up instead.
const registerHooks = (nodeModule as Partial<typeof nodeModule>).registerHooks
if (typescript && registerHooks) {
  registerHooks({
    resolve(specifier, context, next) {
      if (specifier === 'typescript') return { url: typescript, format: 'commonjs', shortCircuit: true }
      return next(specifier, context)
    }
  })
}

try {
  await import('./serve.ts')
} catch (error) {
  const missing = error instanceof Error && 'code' in error && error.code === 'ERR_MODULE_NOT_FOUND' && /typescript/.test(error.message)
  protocol(`${JSON.stringify({
    event: 'error',
    message: missing
      ? 'Haunt has no TypeScript of its own here. Reinstall it, or trust this folder to use the project\'s.'
      : error instanceof Error ? error.stack ?? error.message : String(error)
  })}\n`)
  process.exitCode = 1
}
