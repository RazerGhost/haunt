import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cloneName, repoTree, type RepoTree } from './repositories.ts'

/* The repository manager's folder tree, and a clone's default folder name. */

const names = (node: RepoTree): unknown => node.repo ? node.name : { [node.name]: node.children.map(names) }

test('puts the repositories of a folder in a tree, folding a folder that holds only one', () => {
  const tree = repoTree('C:\\Projects', ['C:\\Projects\\haunt', 'C:\\Projects\\client\\site', 'C:\\Projects\\work\\api', 'C:\\Projects\\work\\web'])
  assert.deepEqual(names(tree), { Projects: [{ work: ['api', 'web'] }, 'client/site', 'haunt'] })
  assert.equal(tree.children.find(child => child.name === 'client/site')?.repo, 'C:\\Projects\\client\\site')
})

test('folds only folders, never a repository into its folder', () => {
  const tree = repoTree('/home/me/src', ['/home/me/src/app', '/home/me/src/tools/a', '/home/me/src/tools/b'])
  assert.deepEqual(names(tree), { src: [{ tools: ['a', 'b'] }, 'app'] })
})

test('names a clone as git does', () => {
  assert.equal(cloneName('https://github.com/owner/haunt.git'), 'haunt')
  assert.equal(cloneName('git@github.com:owner/haunt.git'), 'haunt')
  assert.equal(cloneName('git@host:haunt'), 'haunt')
  assert.equal(cloneName('https://example.com/group/app/'), 'app')
  assert.equal(cloneName('C:\\repos\\tool'), 'tool')
  assert.equal(cloneName(''), '')
})
