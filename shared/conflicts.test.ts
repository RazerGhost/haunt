import assert from 'node:assert/strict'
import { test } from 'node:test'
import { choose, conflictsLeft, joinSegments, parseConflicts } from './conflicts.ts'

/* A conflicted file: its blocks read from git's markers, chosen one by one, and written back. */

const file = [
  'one\n',
  '<<<<<<< HEAD\n',
  'mine\n',
  '=======\n',
  'yours\n',
  '>>>>>>> feature\n',
  'two\n',
  '<<<<<<< HEAD\n',
  'a\n',
  '||||||| base\n',
  'o\n',
  '=======\n',
  'b\n',
  '>>>>>>> feature\n'
].join('')

test('reads plain text and conflict blocks, the diff3 base too', () => {
  const segments = parseConflicts(file)
  assert.equal(segments.length, 4)
  assert.deepEqual(segments[0], { kind: 'text', text: 'one\n' })
  assert.deepEqual(segments[1], { kind: 'conflict', ours: 'mine\n', base: null, theirs: 'yours\n', oursLabel: 'HEAD', baseLabel: '', theirsLabel: 'feature' })
  assert.equal(segments[3]!.kind === 'conflict' && segments[3]!.base, 'o\n')
  assert.equal(conflictsLeft(segments), 2)
  // Unchosen, the file comes back as it was.
  assert.equal(joinSegments(segments), file)
})

test('writes back the chosen lines: yours, theirs or both', () => {
  let segments = parseConflicts(file)
  segments = choose(segments, 1, 'theirs')
  assert.equal(conflictsLeft(segments), 1)
  segments = choose(segments, 0, 'both')
  assert.equal(joinSegments(segments), 'one\nmine\nyours\ntwo\nb\n')
  assert.equal(joinSegments(choose(parseConflicts(file), 0, 'ours')).startsWith('one\nmine\ntwo\n'), true)
})

test('keeps Windows line endings and leaves a lone marker alone', () => {
  const windows = 'x\r\n<<<<<<< HEAD\r\nmine\r\n=======\r\nyours\r\n>>>>>>> other\r\n'
  const segments = parseConflicts(windows)
  assert.equal(joinSegments(segments), windows)
  assert.equal(joinSegments(choose(segments, 0, 'ours')), 'x\r\nmine\r\n')
  // A start marker without an end is just text (a Markdown file showing an example, say).
  assert.deepEqual(parseConflicts('<<<<<<< not a conflict\ntext\n'), [{ kind: 'text', text: '<<<<<<< not a conflict\ntext\n' }])
})
