/*
 * Links to a remote's pages on the web, in the shape each host uses: a commit, a file at a commit (and a line, or a
 * range of lines such as a function's), and a branch. The remote's project page comes from Rust (git/remote.rs); paths here are relative to the opened folder, and
 * `prefix` puts them back in the repository.
 */

export type WebKind = 'github' | 'gitlab' | 'bitbucket' | 'gitea'
export interface Web { base: string, kind: WebKind, prefix: string }

const encodePath = (path: string) => path.split('/').map(encodeURIComponent).join('/')

export function commitLink(web: Web, sha: string): string {
  if (web.kind === 'gitlab') return `${web.base}/-/commit/${sha}`
  if (web.kind === 'bitbucket') return `${web.base}/commits/${sha}`
  return `${web.base}/commit/${sha}`
}

export function fileLink(web: Web, at: string, path: string, line?: number, endLine?: number): string {
  const file = encodePath(`${web.prefix}${path}`)
  const range = !!line && !!endLine && endLine > line
  if (web.kind === 'gitlab') return `${web.base}/-/blob/${at}/${file}${line ? `#L${line}${range ? `-${endLine}` : ''}` : ''}`
  if (web.kind === 'bitbucket') return `${web.base}/src/${at}/${file}${line ? `#lines-${line}${range ? `:${endLine}` : ''}` : ''}`
  const lines = line ? `#L${line}${range ? `-L${endLine}` : ''}` : ''
  if (web.kind === 'gitea') return `${web.base}/src/commit/${at}/${file}${lines}`
  return `${web.base}/blob/${at}/${file}${lines}`
}

export function branchLink(web: Web, branch: string): string {
  const name = encodePath(branch)
  if (web.kind === 'gitlab') return `${web.base}/-/tree/${name}`
  if (web.kind === 'bitbucket') return `${web.base}/branch/${name}`
  if (web.kind === 'gitea') return `${web.base}/src/branch/${name}`
  return `${web.base}/tree/${name}`
}

/** "GitHub", "GitLab"… for the menus. */
export function hostName(kind: WebKind): string {
  return { github: 'GitHub', gitlab: 'GitLab', bitbucket: 'Bitbucket', gitea: 'Gitea' }[kind]
}
