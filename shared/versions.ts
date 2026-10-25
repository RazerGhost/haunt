/** -1, 0 or 1 as `a` is older than, the same as, or newer than `b`: `1.10.0` after `1.9.2`; a leading v and anything after a dash are left out. */
export function compareVersions(a: string, b: string): number {
  const parts = (version: string) => version.replace(/^v/, '').split('-')[0]!.split('.').map(part => Number.parseInt(part, 10) || 0)
  const [x, y] = [parts(a), parts(b)]
  for (let at = 0; at < Math.max(x.length, y.length); at++) {
    const diff = (x[at] ?? 0) - (y[at] ?? 0)
    if (diff) return Math.sign(diff)
  }
  return 0
}
