import { createInterface } from 'node:readline'
import { fingerprint, readCalls, writeCalls } from './cache.ts'
import { scanner, type OtherFiles } from './scan.ts'
import type { CallEdge, Scan } from '../shared/types.ts'
import type { Uses } from './calls.ts'

/*
 * The sidecar's side of the protocol (started through main.ts). One JSON message per line, both ways.
 *
 * From the app: { "type": "scan" } scans (requests arriving while it works fold into one more scan). Another version of a
 * file's functions, for the changes by function, comes from the reader (read.ts), which never waits behind a scan.
 * To the app, unasked: { "event": "structure", "scan" }, then { "event": "calls", "scannedAt", "calls", "uses", "ms" }, or
 * { "event": "error", "message" }.
 * Asked by the app, any time after the first structure: { "id", "type": "imports", "files": [{ "key", "path", "text" }] },
 * the imports of other versions of files, resolved and judged (scan.ts `imports`); it answers { "id", "ok", "value" }
 * or { "id", "ok": false, "error" }. And { "id", "type": "routes" }: the HTTP routes in the code (routes.ts). Both are
 * answered while a call graph is being worked out too: it gives way every few milliseconds (calls.ts).
 * And asked of the app while scanning: { "ask", "type": "read-others", "files", "cargoManifests" } for the PHP, Rust and
 * Lua files, which Rust reads (src-tauri/src/code); it answers { "answer", "ok", "value" | "error" }.
 */

export type Event =
  | { event: 'structure', scan: Scan }
  | { event: 'calls', scannedAt: string, calls: CallEdge[], uses: Uses, ms: number }
  | { event: 'error', message: string }

type Answer = { answer: number, ok: true, value: unknown } | { answer: number, ok: false, error: string }

const writer = (globalThis as { hauntWrite?: (line: string) => void }).hauntWrite
const root = process.argv[2] ?? ''
if (!root || !writer) throw new Error('Start it through main.ts: node sidecar/main.ts <repository>')
const write = writer
const send = (message: Event) => write(JSON.stringify(message))

/* Asking the app */

let asked = 0
const answers = new Map<number, { done: (value: unknown) => void, failed: (error: Error) => void }>()
function ask<T>(type: string, payload: object): Promise<T> {
  const id = ++asked
  return new Promise<T>((done, failed) => {
    answers.set(id, { done: value => done(value as T), failed })
    write(JSON.stringify({ ask: id, type, ...payload }))
  })
}

const readOthers = (files: { path: string, test: boolean }[], cargoManifests: string[]) =>
  ask<OtherFiles>('read-others', { files, cargoManifests })
const trusted = process.env.HAUNT_TRUSTED === '1'
const scan = scanner(root, { trusted, readOthers })

let busy = false
let again = false

async function run(): Promise<void> {
  if (busy) {
    again = true
    return
  }
  busy = true
  do {
    again = false
    try {
      const structure = await scan.structure()
      send({ event: 'structure', scan: structure })
      // Let a newer request in before the long part: no use working out calls for files that changed again.
      await new Promise(resolve => setTimeout(resolve, 50))
      if (again) continue
      // The call graph saved at an earlier launch, when nothing it was worked out from changed (cache.ts).
      const key = fingerprint(root, structure.files.map(file => file.path), trusted, scan.settings())
      const saved = readCalls(process.env.HAUNT_CACHE, root, key)
      if (saved) {
        send({ event: 'calls', scannedAt: structure.scannedAt, calls: saved.calls, uses: saved.uses, ms: 0 })
        continue
      }
      const started = performance.now()
      // In turns: the app's questions (imports, routes) are answered meanwhile, and a newer scan stops it.
      const graph = await scan.callsInTurns(() => again)
      if (!graph) continue
      const { calls, uses } = graph
      const ms = Math.round(performance.now() - started)
      // Saved before it is sent: the app may stop this process (a closed tab) as soon as it has the graph.
      writeCalls(process.env.HAUNT_CACHE, root, key, { calls, uses })
      send({ event: 'calls', scannedAt: structure.scannedAt, calls, uses, ms })
    } catch (error) {
      send({ event: 'error', message: error instanceof Error ? error.stack ?? error.message : String(error) })
    }
  } while (again)
  busy = false
}

const lines = createInterface({ input: process.stdin, crlfDelay: Infinity })
lines.on('line', (line) => {
  if (!line.trim()) return
  let message: Answer | { type: 'scan' } | { id: number, type: 'imports', files: { key: string, path: string, text: string }[] } | { id: number, type: 'routes' }
  try {
    message = JSON.parse(line)
  } catch {
    return
  }
  if ('answer' in message) {
    const waiting = answers.get(message.answer)
    answers.delete(message.answer)
    if (message.ok) waiting?.done(message.value)
    else waiting?.failed(new Error(message.error))
  } else if (message.type === 'scan') {
    void run()
  } else if (message.type === 'imports' || message.type === 'routes') {
    try {
      const value = message.type === 'imports' ? scan.imports(message.files) : scan.routes()
      write(JSON.stringify({ id: message.id, ok: true, value }))
    } catch (error) {
      write(JSON.stringify({ id: message.id, ok: false, error: error instanceof Error ? error.message : String(error) }))
    }
  }
})
// The app went away (stdin closed): nothing to scan for.
lines.on('close', () => process.exit(0))
