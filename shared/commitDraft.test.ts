import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftCommitMessage, functionPhrases, partLabel, type DraftFile } from './commitDraft.ts'

const file = (path: string, part: string | null, functions: DraftFile['functions'], status: DraftFile['status'] = 'modified'): DraftFile =>
  ({ path, oldPath: path, status, part, code: true, functions })

test('names the part by its last folder', () => {
  assert.equal(partLabel('src/features/entries'), 'Entries')
  assert.equal(partLabel('api'), 'Api')
})

test('says what functions a change adds, changes, renames, moves and removes', () => {
  assert.deepEqual(functionPhrases([
    { name: 'resolveModel', status: 'added' },
    { name: 'createEntry', status: 'changed' },
    { name: 'assist', status: 'moved', from: { name: 'helper' } },
    { name: 'same', status: 'moved', from: { name: 'same' } },
    { name: 'old', status: 'removed' },
    { name: '(anonymous)', status: 'changed' }
  ]), ['adds resolveModel', 'changes createEntry', 'renames helper to assist', 'moves same', 'removes old'])
  assert.deepEqual(functionPhrases(['a', 'b', 'c'].map(name => ({ name, status: 'changed' as const })), 2), ['changes 3 functions'])
})

test('drafts a subject from the main part, with the callers outside the change', () => {
  const draft = draftCommitMessage([
    file('src/features/entries/server/entries.ts', 'src/features/entries', [{ name: 'createEntry', status: 'changed' }, { name: 'resolveModel', status: 'added' }]),
    file('README.md', null, [])
  ], new Map([['apps/api', 3]]))
  assert.equal(draft.subject, 'Entries: adds resolveModel; changes createEntry; 3 callers in Api')
  assert.equal(draft.body, [
    '- src/features/entries/server/entries.ts: adds resolveModel, changes',
    '  createEntry',
    '- README.md: changed',
    '',
    'Called from outside the change: 3 callers in Api.'
  ].join('\n'))
})

test('keeps the subject within 72 characters by counting, then dropping phrases', () => {
  const many = ['firstLongFunctionName', 'secondLongFunctionName', 'thirdLongFunctionName']
  const draft = draftCommitMessage([file('a/app/x.ts', 'app', [
    ...many.map(name => ({ name, status: 'added' as const })),
    ...many.map(name => ({ name: `${name}Two`, status: 'changed' as const }))
  ])])
  assert.equal(draft.subject, 'App: adds 3 functions; changes 3 functions')
  assert.ok(draft.subject.length <= 72)
  assert.ok(draft.body.split('\n').every(line => line.length <= 72))
})

test('names files when no function changed', () => {
  assert.equal(draftCommitMessage([file('docs/guide.md', null, [], 'added'), { ...file('b.txt', null, [], 'renamed'), oldPath: 'a.txt' }]).subject, 'adds guide.md; renames a.txt to b.txt')
  assert.deepEqual(draftCommitMessage([]), { subject: '', body: '' })
})
