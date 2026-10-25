/*
 * Whether a new import would close a cycle: `from` importing `to` closes one when `to` already reaches `from` through
 * the imports there are (run-time imports only: type-only ones are gone at run time and make no cycle).
 */

export function importGraph(edges: Iterable<{ from: string, to: string, typeOnly: boolean }>): Map<string, string[]> {
  const graph = new Map<string, string[]>()
  for (const edge of edges) {
    if (edge.typeOnly) continue
    const list = graph.get(edge.from) ?? graph.set(edge.from, []).get(edge.from)!
    if (!list.includes(edge.to)) list.push(edge.to)
  }
  return graph
}

/** Whether `start` reaches `target` through `graph`. */
export function reaches(graph: Map<string, string[]>, start: string, target: string): boolean {
  if (start === target) return true
  const seen = new Set([start])
  const queue = [start]
  // Read from the front by position: shifting would move every item along each time.
  for (let at = 0; at < queue.length; at++) {
    for (const next of graph.get(queue[at]!) ?? []) {
      if (next === target) return true
      if (!seen.has(next)) {
        seen.add(next)
        queue.push(next)
      }
    }
  }
  return false
}
