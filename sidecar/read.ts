import { createInterface } from 'node:readline'
import { versionFunctions } from './version.ts'

/*
 * The reader: `node sidecar/read.ts`, one for the app, started by Rust (src-tauri/src/repos.rs). It reads the functions
 * of a TypeScript-family file's text (another version of it, at a commit, for the changes by function), as the scan
 * reads the file. It scans nothing, so it answers at once even while a repository's sidecar works out a call graph, and
 * it uses Haunt's own TypeScript only: it runs nothing of any repository's.
 *
 * One JSON message per line: { "id", "type": "functions", "root", "path", "text" } in, { "id", "ok": true, "value" } or
 * { "id", "ok": false, "error" } out.
 */

type Request = { id: number, type: 'functions', root: string, path: string, text: string }

const lines = createInterface({ input: process.stdin, crlfDelay: Infinity })
lines.on('line', (line) => {
  let request: Request
  try {
    request = JSON.parse(line)
  } catch {
    return
  }
  if (request.type !== 'functions') return
  let reply: object
  try {
    reply = { id: request.id, ok: true, value: versionFunctions(request.root, request.path, request.text) }
  } catch (error) {
    reply = { id: request.id, ok: false, error: (error instanceof Error ? error.message : String(error)).split('\n')[0] || 'Failed.' }
  }
  process.stdout.write(`${JSON.stringify(reply)}\n`)
})
// The app went away (stdin closed).
lines.on('close', () => process.exit(0))
