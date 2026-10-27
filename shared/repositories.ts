/*
 * The repository manager's lists: the repositories found in a folder as a tree of the folders between them (one
 * child folder holding one repository and nothing else is folded into it, as "client/site"), and the folder name a
 * clone gets by default, as git gives it.
 */

export interface RepoTree {
  /** The folders on the way, joined with "/" when folded ("client/site"); the folder's own name at the top. */
  name: string
  /** The repository here, when this is one. */
  repo: string | null
  children: RepoTree[]
}

const parts = (path: string) => path.split(/[\\/]/).filter(Boolean)

/** The repositories under `folder` as a tree; `repos` are full paths inside it. */
export function repoTree(folder: string, repos: string[]): RepoTree {
  const top: RepoTree = { name: parts(folder).pop() ?? folder, repo: null, children: [] }
  const base = parts(folder)
  for (const repo of repos) {
    const inside = parts(repo).slice(base.length)
    let at = top
    inside.forEach((name, index) => {
      if (index === inside.length - 1) {
        at.children.push({ name, repo, children: [] })
        return
      }
      let child = at.children.find(known => known.name === name && known.repo === null)
      if (!child) {
        child = { name, repo: null, children: [] }
        at.children.push(child)
      }
      at = child
    })
  }
  const fold = (node: RepoTree): RepoTree => {
    node.children = node.children.map(fold)
    const only = node.children[0]
    if (node !== top && node.repo === null && node.children.length === 1 && only) {
      return { ...only, name: `${node.name}/${only.name}` }
    }
    node.children.sort((a, b) => Number(a.repo !== null) - Number(b.repo !== null) || a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
    return node
  }
  return fold(top)
}

/** The folder a clone of `url` goes into by default: the last part of its path, without ".git". */
export function cloneName(url: string): string {
  const path = url.trim().replace(/[\\/]+$/, '')
  const last = path.split(/[\\/:]/).pop() ?? ''
  return last.replace(/\.git$/i, '')
}
