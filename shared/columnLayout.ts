/*
 * The code graph's cards in columns that mean something (app/utils/cardLayout.ts): the project's layers left to
 * right, or a function's callers left of it and what it calls right of it. Within a column the cards are ordered so the
 * lines between columns cross less (each card moved towards the average place of the cards it is joined to, a few
 * sweeps each way), a column taller than the room is wrapped into several, and every column is centred on the middle.
 */

export interface ColumnNode {
  id: string
  /** Which column, any integer: smaller is further left. */
  column: number
  w: number
  h: number
}

export interface ColumnLink { source: string, target: string }

export interface ColumnOptions {
  /** Between cards in a column. */
  gap?: number
  /** Between columns, room for the lines. */
  columnGap?: number
  /** A column taller than this wraps into more columns side by side. */
  maxHeight?: number
  sweeps?: number
}

export interface Placed { x: number, y: number, w: number, h: number }

/** Each node's top-left corner and size. Nodes keep the order they are given in, until the sweeps move them. */
export function columnLayout(nodes: ColumnNode[], links: ColumnLink[], options: ColumnOptions = {}): Map<string, Placed> {
  const gap = options.gap ?? 20
  const columnGap = options.columnGap ?? 120
  const maxHeight = options.maxHeight ?? 1400
  const sweeps = options.sweeps ?? 4

  const byColumn = new Map<number, ColumnNode[]>()
  for (const node of nodes) byColumn.set(node.column, [...byColumn.get(node.column) ?? [], node])
  const columns = [...byColumn.keys()].sort((a, b) => a - b).map(key => byColumn.get(key)!)
  const columnOf = new Map(nodes.map(node => [node.id, node.column]))

  const neighbours = new Map<string, string[]>()
  const join = (a: string, b: string) => neighbours.set(a, [...neighbours.get(a) ?? [], b])
  for (const link of links) {
    if (link.source === link.target || !columnOf.has(link.source) || !columnOf.has(link.target)) continue
    join(link.source, link.target)
    join(link.target, link.source)
  }

  // Where each node stands in its column, 0 at the top to 1 at the bottom, so columns of any length compare.
  const place = new Map<string, number>()
  const mark = (column: ColumnNode[]) => column.forEach((node, at) => place.set(node.id, column.length > 1 ? at / (column.length - 1) : 0.5))
  columns.forEach(mark)
  const reorder = (column: ColumnNode[], side: (other: number, own: number) => boolean) => {
    const wanted = new Map<string, number>()
    for (const node of column) {
      const own = node.column
      const near = (neighbours.get(node.id) ?? []).filter(id => side(columnOf.get(id)!, own))
      wanted.set(node.id, near.length ? near.reduce((sum, id) => sum + place.get(id)!, 0) / near.length : place.get(node.id)!)
    }
    // A stable sort: nodes with equal wishes keep their order.
    column.sort((a, b) => wanted.get(a.id)! - wanted.get(b.id)!)
    mark(column)
  }
  for (let sweep = 0; sweep < sweeps; sweep++) {
    for (const column of columns.slice(1)) reorder(column, (other, own) => other < own)
    for (const column of columns.slice(0, -1).reverse()) reorder(column, (other, own) => other > own)
  }

  // Tall columns wrapped, then each column placed, centred on the middle.
  const placed = new Map<string, Placed>()
  let x = 0
  for (const column of columns) {
    const total = column.reduce((sum, node) => sum + node.h, 0) + gap * (column.length - 1)
    const parts = Math.max(1, Math.ceil(total / maxHeight))
    const per = Math.ceil(column.length / parts)
    for (let part = 0; part < parts; part++) {
      const slice = column.slice(part * per, (part + 1) * per)
      if (!slice.length) continue
      const height = slice.reduce((sum, node) => sum + node.h, 0) + gap * (slice.length - 1)
      const width = Math.max(...slice.map(node => node.w))
      let y = -height / 2
      for (const node of slice) {
        placed.set(node.id, { x, y, w: node.w, h: node.h })
        y += node.h + gap
      }
      x += width + (part < parts - 1 ? gap * 2 : columnGap)
    }
  }
  return placed
}
