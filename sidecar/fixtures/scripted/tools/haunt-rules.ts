// This fixture's rules in code: the interface on top of the logic, and the logic may not import the interface.
// It leaves a mark when it runs, so the tests can see that an untrusted folder never runs it.
(globalThis as { hauntFixtureRuns?: number }).hauntFixtureRuns = ((globalThis as { hauntFixtureRuns?: number }).hauntFixtureRuns ?? 0) + 1

interface Side { part: string, path: string }

export default function architecture({ files }: { root: string, files: string[] }) {
  if (!files.includes('src/core/sum.ts')) throw new Error('Haunt did not pass the scanned files')
  return {
    layers: [{ id: 'ui', label: 'Interface' }, { id: 'core', note: 'the logic' }],
    parts: [{ name: 'ui', layer: 'ui' }, { name: 'core', layer: 'core', role: 'adds things up' }],
    partOf: (path: string) => path.startsWith('src/ui/') ? 'ui' : path.startsWith('src/core/') ? 'core' : null,
    judge: (from: Side, to: Side) => from.part === 'core' && to.part === 'ui' ? ['layer'] : []
  }
}
