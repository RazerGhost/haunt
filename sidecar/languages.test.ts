import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { isTestPath, languageOf } from './languages.ts'

/* The language table agrees with the app's copy (src-tauri/src/code/languages.rs), which is tested on the same cases. */

test('reads the shared cases as the app does', () => {
  const cases = JSON.parse(readFileSync(new URL('../shared/languages.cases.json', import.meta.url), 'utf8')) as [string, string | null][]
  for (const [path, expected] of cases) assert.equal(languageOf(path), expected, path)
})

test('knows tests by their names and folders, test helpers in a testing folder too', () => {
  for (const path of ['a.test.ts', 'src/a.spec.tsx', 'tests/a.ts', 'src/testing/access-sweep.ts', 'e2e/login.ts', 'UserTest.php', 'x_spec.lua', 'src/OrderTest.java', 'OrderTests.cs', 'order_spec.rb', 'order_test.cpp']) {
    assert.equal(isTestPath(path), true, path)
  }
  for (const path of ['src/a.ts', 'src/kernel/testable.ts', 'docs/testing.md', 'src/Order.java']) assert.equal(isTestPath(path), false, path)
})
