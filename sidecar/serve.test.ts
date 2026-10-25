import assert from 'node:assert/strict'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

/* The sidecar's and the reader's protocol: one JSON message per line over stdin and stdout, as the app (sidecar.rs)
 * speaks it. */

const main = fileURLToPath(new URL('./main.ts', import.meta.url))
const reader = fileURLToPath(new URL('./read.ts', import.meta.url))
const fixture = fileURLToPath(new URL('./fixtures/single', import.meta.url))
const laravel = fileURLToPath(new URL('./fixtures/laravel', import.meta.url))

interface Sidecar {
  send: (message: object) => void
  next: (match: (message: Record<string, unknown>) => boolean) => Promise<Record<string, unknown>>
  /** Every stdout line seen so far, parsed (a line that is not JSON fails the test). */
  seen: Record<string, unknown>[]
  child: ChildProcessWithoutNullStreams
}

function start(root: string, script = main, env: Record<string, string> = {}): Sidecar {
  const child = spawn(process.execPath, [script, root], { env: { ...process.env, HAUNT_TRUSTED: '0', ...env } })
  const seen: Record<string, unknown>[] = []
  const waiting: { match: (message: Record<string, unknown>) => boolean, done: (message: Record<string, unknown>) => void }[] = []
  createInterface({ input: child.stdout }).on('line', (line) => {
    const message = JSON.parse(line) as Record<string, unknown>
    seen.push(message)
    for (const wait of [...waiting]) {
      if (wait.match(message)) {
        waiting.splice(waiting.indexOf(wait), 1)
        wait.done(message)
      }
    }
  })
  return {
    child,
    seen,
    send: message => child.stdin.write(`${JSON.stringify(message)}\n`),
    next: match => new Promise((done) => {
      const found = seen.find(match)
      if (found) done(found)
      else waiting.push({ match, done })
    })
  }
}

test('scans on request: the structure, then the calls', async () => {
  const sidecar = start(fixture)
  try {
    sidecar.send({ type: 'scan' })
    const structure = await sidecar.next(message => message.event === 'structure')
    const scan = structure.scan as { files: { path: string }[], scannedAt: string }
    assert.ok(scan.files.some(file => file.path.endsWith('.ts')))
    const calls = await sidecar.next(message => message.event === 'calls')
    assert.equal(calls.scannedAt, scan.scannedAt)
    assert.ok(Array.isArray(calls.calls))
  } finally {
    sidecar.child.kill()
  }
})

test('the reader answers each request once, by its id: the functions of a file\'s text', async () => {
  const sidecar = start('', reader)
  try {
    sidecar.send({ id: 1, type: 'functions', root: fixture, path: 'src/a.ts', text: 'export function f() {\n  return 1\n}\n' })
    // Lines that are not JSON, or not a known message, are ignored rather than ending the reader.
    sidecar.child.stdin.write('not json\n')
    sidecar.send({ id: 2, type: 'functions', root: fixture, path: 'src/b.php', text: '<?php function g() {}' })
    const ts = await sidecar.next(message => message.id === 1)
    assert.equal(ts.ok, true)
    assert.deepEqual((ts.value as { name: string, line: number }[]).map(fn => `${fn.name}:${fn.line}`), ['f:1'])
    // Another language is the app's to read.
    const php = await sidecar.next(message => message.id === 2)
    assert.deepEqual(php.value, [])
    assert.equal(sidecar.seen.filter(message => message.id === 1).length, 1)
  } finally {
    sidecar.child.kill()
  }
})

test('asks the app for the PHP, Rust and Lua files while it scans, and puts its answer into the scan', async () => {
  const sidecar = start(laravel)
  try {
    sidecar.send({ type: 'scan' })
    const asked = await sidecar.next(message => message.type === 'read-others')
    const files = asked.files as { path: string, test: boolean }[]
    assert.ok(files.some(file => file.path === 'app/Models/Order.php' && !file.test))
    assert.ok(files.some(file => file.path === 'tests/Feature/OrderTest.php' && file.test))
    // Answered as the app would: one function in Order.php.
    const order = { id: 'app/Models/Order.php:5:1', name: 'Order', kind: 'class', line: 5, endLine: 15, exported: true, async: false, anonymous: false, children: [] }
    sidecar.send({ answer: asked.ask, ok: true, value: { files: { 'app/Models/Order.php': { tree: [order], imports: [], lines: 15 } }, calls: [] } })
    const structure = await sidecar.next(message => message.event === 'structure')
    const scanned = (structure.scan as { files: { path: string, functions: { name: string }[], lines: number }[] }).files
    const file = scanned.find(item => item.path === 'app/Models/Order.php')
    assert.deepEqual(file?.functions.map(fn => fn.name), ['Order'])
    assert.equal(file?.lines, 15)
    // A file the answer left out is not in the scan.
    assert.ok(!scanned.some(item => item.path === 'routes/web.php'))
  } finally {
    sidecar.child.kill()
  }
})

test('keeps the call graph for the next launch, and sends it at once while nothing changed', async () => {
  const cache = mkdtempSync(join(tmpdir(), 'haunt-serve-cache-'))
  try {
    const first = start(fixture, main, { HAUNT_CACHE: cache })
    first.send({ type: 'scan' })
    const worked = await first.next(message => message.event === 'calls')
    first.child.kill()
    const second = start(fixture, main, { HAUNT_CACHE: cache })
    second.send({ type: 'scan' })
    const kept = await second.next(message => message.event === 'calls')
    second.child.kill()
    assert.equal(kept.ms, 0)
    assert.deepEqual(kept.calls, worked.calls)
    assert.ok((worked.calls as unknown[]).length > 0)
  } finally {
    rmSync(cache, { recursive: true, force: true })
  }
})

test('ends when the app closes its stdin', async () => {
  const sidecar = start(fixture)
  const exited = new Promise<number | null>(done => sidecar.child.on('exit', done))
  sidecar.child.stdin.end()
  assert.equal(await exited, 0)
})
