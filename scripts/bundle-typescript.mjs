// Copies Haunt's own TypeScript into the desktop app's files (src-tauri/bundled/node_modules), for folders you have not
// trusted, so their own node_modules never runs: package.json, the licence, lib/typescript.js and the lib.*.d.ts files the
// type checker reads. PHP, Rust and Lua need nothing here: the app reads them itself (src-tauri/src/code).
import { copyFileSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const require = createRequire(import.meta.url)
const target = join(here, '../src-tauri/bundled/node_modules')

const source = dirname(require.resolve('typescript/package.json'))
const files = ['package.json', 'LICENSE.txt', 'lib/typescript.js', ...readdirSync(join(source, 'lib')).filter(name => /^lib\..*\.d\.ts$/.test(name)).map(name => `lib/${name}`)]

rmSync(target, { recursive: true, force: true })
let bytes = 0
for (const file of files) {
  const to = join(target, 'typescript', file)
  mkdirSync(dirname(to), { recursive: true })
  copyFileSync(join(source, file), to)
  bytes += statSync(to).size
}
console.log(`Bundled TypeScript: ${files.length} files, ${(bytes / 1024 / 1024).toFixed(1)} MB, in src-tauri/bundled.`)
