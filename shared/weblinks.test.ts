import assert from 'node:assert/strict'
import { test } from 'node:test'
import { branchLink, commitLink, fileLink, issuesLink, pullsLink, type Web } from './weblinks.ts'

/* Links to a commit, a file and a branch, as each host shapes them. */

const github: Web = { base: 'https://github.com/o/r', kind: 'github', prefix: '' }
const gitlab: Web = { base: 'https://gitlab.com/g/r', kind: 'gitlab', prefix: 'packages/app/' }

test('builds GitHub links', () => {
  assert.equal(commitLink(github, 'abc'), 'https://github.com/o/r/commit/abc')
  assert.equal(fileLink(github, 'abc', 'src/a b.ts', 12), 'https://github.com/o/r/blob/abc/src/a%20b.ts#L12')
  assert.equal(fileLink(github, 'abc', 'a.ts', 12, 20), 'https://github.com/o/r/blob/abc/a.ts#L12-L20')
  assert.equal(fileLink(github, 'abc', 'a.ts', 12, 12), 'https://github.com/o/r/blob/abc/a.ts#L12')
  assert.equal(branchLink(github, 'feature/login'), 'https://github.com/o/r/tree/feature/login')
})

test('builds GitLab, Bitbucket and Gitea links, with the folder opened inside the repository', () => {
  assert.equal(commitLink(gitlab, 'abc'), 'https://gitlab.com/g/r/-/commit/abc')
  assert.equal(fileLink(gitlab, 'abc', 'src/a.ts'), 'https://gitlab.com/g/r/-/blob/abc/packages/app/src/a.ts')
  assert.equal(fileLink({ base: 'https://bitbucket.org/t/r', kind: 'bitbucket', prefix: '' }, 'abc', 'a.ts', 3), 'https://bitbucket.org/t/r/src/abc/a.ts#lines-3')
  assert.equal(fileLink({ base: 'https://bitbucket.org/t/r', kind: 'bitbucket', prefix: '' }, 'abc', 'a.ts', 3, 9), 'https://bitbucket.org/t/r/src/abc/a.ts#lines-3:9')
  assert.equal(fileLink(gitlab, 'abc', 'a.ts', 3, 9), 'https://gitlab.com/g/r/-/blob/abc/packages/app/a.ts#L3-9')
  assert.equal(branchLink({ base: 'https://codeberg.org/m/r', kind: 'gitea', prefix: '' }, 'main'), 'https://codeberg.org/m/r/src/branch/main')
})

test('builds the issues and pull requests pages', () => {
  assert.equal(issuesLink(github), 'https://github.com/o/r/issues')
  assert.equal(pullsLink(github), 'https://github.com/o/r/pulls')
  assert.equal(issuesLink(gitlab), 'https://gitlab.com/g/r/-/issues')
  assert.equal(pullsLink(gitlab), 'https://gitlab.com/g/r/-/merge_requests')
  assert.equal(pullsLink({ base: 'https://bitbucket.org/t/r', kind: 'bitbucket', prefix: '' }), 'https://bitbucket.org/t/r/pull-requests')
  assert.equal(pullsLink({ base: 'https://codeberg.org/m/r', kind: 'gitea', prefix: '' }), 'https://codeberg.org/m/r/pulls')
})
