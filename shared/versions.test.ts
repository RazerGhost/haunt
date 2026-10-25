import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compareVersions } from './versions.ts'

test('compares versions part by part, as numbers', () => {
  assert.equal(compareVersions('v1.10.0', '1.9.2'), 1)
  assert.equal(compareVersions('0.1.0', 'v0.1.0'), 0)
  assert.equal(compareVersions('0.1', '0.1.1'), -1)
  assert.equal(compareVersions('1.0.0-beta.2', '1.0.0'), 0)
})
