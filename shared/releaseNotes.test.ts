import assert from 'node:assert/strict'
import { test } from 'node:test'
import { releaseNotes } from './releaseNotes.ts'

const partOfPath = (path: string) => path.startsWith('src/entries/') ? 'src/entries' : path.startsWith('src/api/') ? 'src/api' : null

test('groups the commits by the part they changed most, with what its functions gained and lost', () => {
  const notes = releaseNotes({
    from: 'v1',
    to: 'v2',
    commits: [
      { sha: 'aaaaaaa1', subject: 'Slugs for entries', paths: ['src/entries/entries.ts', 'src/api/routes.ts', 'src/entries/slug.ts'] },
      { sha: 'bbbbbbb2', subject: 'Store route', paths: ['src/api/routes.ts'] },
      { sha: 'ccccccc3', subject: 'Readme', paths: ['README.md'] },
      { sha: 'ddddddd4', subject: 'Drop old helper', paths: ['src/entries/entries.ts'] }
    ],
    files: [
      { path: 'src/entries/entries.ts', oldPath: 'src/entries/entries.ts', status: 'modified', part: 'src/entries', code: true, functions: [{ name: 'createEntry', status: 'changed' }, { name: 'oldHelper', status: 'removed' }] },
      { path: 'src/entries/slug.ts', oldPath: 'src/entries/slug.ts', status: 'added', part: 'src/entries', code: true, functions: [{ name: 'resolveSlug', status: 'added' }] }
    ],
    partOfPath
  })
  assert.equal(notes, [
    '## v1 to v2',
    '',
    '4 commits.',
    '',
    '### Entries',
    '',
    'Adds resolveSlug; changes createEntry; removes oldHelper.',
    '',
    '- Slugs for entries (aaaaaaa)',
    '- Drop old helper (ddddddd)',
    '',
    '### Api',
    '',
    '- Store route (bbbbbbb)',
    '',
    '### Other',
    '',
    '- Readme (ccccccc)',
    ''
  ].join('\n'))
})

test('from the first commit when there is no older tag', () => {
  assert.ok(releaseNotes({ from: null, to: 'v1', commits: [{ sha: 'a', subject: 'First', paths: [] }], files: [], partOfPath }).startsWith('## v1\n\n1 commit.'))
})
