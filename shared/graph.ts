/*
 * The commit graph's lanes, worked out from the commits' parents in list order (children before parents, as
 * `git log --topo-order` gives them). Each lane waits for the next commit expected in it. A commit takes the lane that
 * waits for it (or a free one, for a branch tip); other lanes waiting for it end in it (branches that split off it);
 * its first parent continues its lane, or joins a lane to its left already waiting for that parent; each further
 * parent of a merge gets a lane of its own. Lanes keep their place, so a lane passing a row is a straight line. The
 * state carries over from page to page: add the pages in order.
 */

export interface GraphLine {
  /** Lane at the top of the line, and at its end. */
  from: number
  to: number
  /** top: from the row's top edge to the commit's dot; bottom: from the dot to the bottom edge; through: edge to edge. */
  part: 'top' | 'bottom' | 'through'
  color: number
}

export interface GraphRow {
  /** The commit's lane. */
  lane: number
  color: number
  lines: GraphLine[]
  /** Lanes in use at this row, for its width. */
  width: number
}

export class GraphLayout {
  private lanes: (string | null)[] = []
  private colors: number[] = []
  private nextColor = 0

  private free(): number {
    const at = this.lanes.indexOf(null)
    return at < 0 ? this.lanes.length : at
  }

  /** The rows for the next commits of the list. */
  add(commits: { sha: string, parents: string[] }[]): GraphRow[] {
    return commits.map(commit => this.row(commit))
  }

  private row({ sha, parents }: { sha: string, parents: string[] }): GraphRow {
    const lines: GraphLine[] = []
    let lane = this.lanes.indexOf(sha)
    const waited = lane >= 0
    if (!waited) {
      lane = this.free()
      this.lanes[lane] = sha
      this.colors[lane] = this.nextColor++
    }
    const color = this.colors[lane]!
    if (waited) lines.push({ from: lane, to: lane, part: 'top', color })
    // Other lanes waiting for this commit end in it.
    this.lanes.forEach((waiting, other) => {
      if (other === lane || waiting !== sha) return
      lines.push({ from: other, to: lane, part: 'top', color: this.colors[other]! })
      this.lanes[other] = null
    })
    // Lanes that pass this row.
    this.lanes.forEach((waiting, other) => {
      if (other !== lane && waiting !== null) lines.push({ from: other, to: other, part: 'through', color: this.colors[other]! })
    })
    const [first, ...others] = parents
    if (first === undefined) {
      this.lanes[lane] = null
    } else {
      // Joining only leftwards keeps the main line in the leftmost lane; a lane to the right ends in the parent instead.
      const joins = this.lanes.findIndex((waiting, other) => other < lane && waiting === first)
      if (joins >= 0) {
        lines.push({ from: lane, to: joins, part: 'bottom', color })
        this.lanes[lane] = null
      } else {
        this.lanes[lane] = first
        lines.push({ from: lane, to: lane, part: 'bottom', color })
      }
    }
    for (const parent of others) {
      let target = this.lanes.indexOf(parent)
      if (target < 0) {
        target = this.free()
        this.lanes[target] = parent
        this.colors[target] = this.nextColor++
      }
      lines.push({ from: lane, to: target, part: 'bottom', color: this.colors[target]! })
    }
    const width = Math.max(lane + 1, ...lines.map(line => Math.max(line.from, line.to) + 1))
    while (this.lanes.length && this.lanes.at(-1) === null) {
      this.lanes.pop()
      this.colors.pop()
    }
    return { lane, color, lines, width }
  }
}
