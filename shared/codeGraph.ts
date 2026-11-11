/*
 * The code graph (app/pages/graph.vue): files as cards with their functions as rows, and the calls between the rows; or,
 * for the whole project, parts as cards with their files as rows and the imports between them. It never draws all of a
 * repository: it starts from a focus (a function, a file, a part, or the project) and shows what is reached from it, a
 * card per file, with only the rows that matter in each card and the rest folded away ("N more · Show").
 *
 * This is the pure half: which cards, which rows, which edges. The page lays the cards out (app/utils/cardLayout.ts)
 * and CodeGraph.vue draws them.
 */
import type { CallEdge, FunctionNode, InnerImport, Part, PartImport, ScannedFile } from './types.ts'

export interface GraphIndex {
  functions: Map<string, { fn: FunctionNode, file: ScannedFile, parent: string | null }>
  callees: Map<string, CallEdge[]>
  callers: Map<string, CallEdge[]>
  filesByPart: Map<string, ScannedFile[]>
}

export type Focus =
  | { kind: 'function', id: string }
  | { kind: 'file', path: string }
  | { kind: 'part', name: string }
  | { kind: 'project' }

export type Direction = 'both' | 'callers' | 'callees'

export interface GraphRow {
  /** A function's id, a file's path (in the project's cards), or a place to fill in (Predict). */
  id: string
  kind: 'function' | 'file' | 'slot'
  /** How deep it sits among the rows of its card (a method under its class). */
  depth: number
  /** Shown only to place the rows under it (the class of a method): no edge of its own. */
  context: boolean
}

export interface GraphCard {
  /** `file:<path>`, `part:<name>` (a part, or the files of one outside the focus), `more:<part>`, `idle:<part>` (the
   * focus's files on no line), or a page's own (Predict's places). */
  id: string
  kind: 'file' | 'part' | 'more' | 'idle' | 'slot'
  /** The file's path, the part's name. */
  name: string
  rows: GraphRow[]
  /** Rows left out of the card: shown by opening it. */
  folded: number
  /** Drawn as its header only; its edges end at the header. */
  shut: boolean
  /** Shows every function of the file, not only the ones on an edge. */
  open: boolean
  /** In the focus (the function's file, the file, the part's files), not reached from it. */
  home: boolean
  /** For `more`: the files left out of that part. */
  count?: number
  /** Where it goes, left to right (shared/columnLayout.ts): its layer in the project, its steps from the function in
   * the middle (callers negative); none where the layout works it out itself. */
  column?: number
}

/** How an import keeps the project's rules: breaks one (new), breaks one the project accepts for now (known), keeps
 * them (ok); plain without rules and for calls. */
export type Tone = 'plain' | 'new' | 'known' | 'ok'

export interface GraphEdge {
  id: string
  /** A row's id, or a card's id when the row is not shown (a shut card). */
  from: string
  to: string
  kind: 'call' | 'import'
  /** Call sites, or file imports, added up. */
  count: number
  /** Every call in it matched by name only. */
  guessed: boolean
  /** Every import in it is of types only. */
  typeOnly: boolean
  tone: Tone
}

export interface CodeGraph {
  cards: GraphCard[]
  edges: GraphEdge[]
  /** Calls between two rows of one card: not drawn as lines between cards, only marked on the rows (and drawn on hover). */
  inner: GraphEdge[]
  /** Files reached but not drawn (over the limit), in the `more` cards or not even there. */
  leftOut: number
}

export interface ImportOptions {
  between: PartImport[]
  inner: InnerImport[] | null
  /** Include imports of types only. */
  typeOnly: boolean
  /** The project has rules: imports get a tone. */
  rules: boolean
  /** The project view: only the imports that break a rule. */
  onlyBreaks?: boolean
}

export interface GraphOptions {
  /** Steps of calls out from the focus. */
  depth?: number
  direction?: Direction
  /** Functions to show (test code left out unless asked for); anonymous callbacks never take part. */
  keep?: (id: string) => boolean
  /** Files to show. */
  keepFile?: (file: ScannedFile) => boolean
  /** Cards the viewer opened ("Show") or closed again: flips what the card starts as. */
  opened?: ReadonlySet<string>
  /** Cards the viewer shut or unshut: flips what the card starts as. */
  flipped?: ReadonlySet<string>
  /** Import edges between the cards, when asked for. */
  imports?: ImportOptions | null
  maxCards?: number
  maxRows?: number
  /** The project's layers in order, for the project's columns. */
  layerOrder?: string[]
}

export const MAX_CARDS = 40
export const MAX_ROWS = 400
/** The project view starts with its part cards shut past this many file rows. */
export const PROJECT_OPEN_ROWS = 120
/** At most this many "more files in …" cards. */
const MAX_MORE = 8

export const fileCard = (path: string) => `file:${path}`
export const partCard = (name: string) => `part:${name}`

/** Every function of a file, depth first, in the order they are written. */
function walk(items: FunctionNode[], visit: (fn: FunctionNode) => void): void {
  for (const fn of items) {
    visit(fn)
    walk(fn.children, visit)
  }
}

/** The functions reached from `seeds`, each weighed by the call sites that reach it (less with every step); the seeds
 * weigh most. */
export function reach(index: GraphIndex, seeds: string[], depth: number, direction: Direction, keep: (id: string) => boolean, steps?: Map<string, number>): Map<string, number> {
  const weight = new Map<string, number>(seeds.map(id => [id, Infinity]))
  for (const id of seeds) steps?.set(id, 0)
  const go = (next: (id: string) => CallEdge[], other: (edge: CallEdge) => string, sign: 1 | -1) => {
    const seen = new Set(seeds)
    let frontier = seeds
    for (let step = 1; step <= depth && frontier.length; step++) {
      const found = new Map<string, number>()
      for (const id of frontier) {
        for (const edge of next(id)) {
          const target = other(edge)
          if (seen.has(target) || !index.functions.has(target) || !keep(target)) continue
          found.set(target, (found.get(target) ?? 0) + edge.count)
        }
      }
      for (const [id, count] of found) {
        seen.add(id)
        weight.set(id, Math.max(weight.get(id) ?? 0, count / step))
        if (steps && !steps.has(id)) steps.set(id, sign * step)
      }
      frontier = [...found.keys()]
    }
  }
  if (direction !== 'callers') go(id => index.callees.get(id) ?? [], edge => edge.to, 1)
  if (direction !== 'callees') go(id => index.callers.get(id) ?? [], edge => edge.from, -1)
  return weight
}

interface Assembly {
  /** The functions to draw, with their weight. */
  members: Map<string, number>
  /** The files of the focus, drawn even without a member. */
  home: ScannedFile[]
  /** Home cards show every function. */
  homeOpen?: boolean
  /** Cards outside the focus start shut. */
  othersShut?: boolean
  /** Home cards show only members on an edge. */
  connectedOnly?: boolean
  /** The edges to draw instead of the calls between members (a path view). */
  edges?: { from: string, to: string }[]
  /** Files outside the focus to draw as well (what a file imports). */
  extra?: ScannedFile[]
  /** Files outside the focus as rows of one card per part, not a card each. */
  groupOthers?: boolean
  /** The focus's files on no line, as rows of one card. */
  idleTogether?: string
  /** Each function's column (its steps from the focus): a file's card goes where its heaviest function does. */
  columns?: Map<string, number>
}

/** The cards, rows and edges for a set of functions. */
export function assemble(index: GraphIndex, plan: Assembly, options: GraphOptions = {}): CodeGraph {
  const keep = (id: string) => (options.keep ? options.keep(id) : true) && !index.functions.get(id)?.fn.anonymous
  const keepFile = options.keepFile ?? (() => true)
  const maxCards = options.maxCards ?? MAX_CARDS
  const maxRows = options.maxRows ?? MAX_ROWS
  const opened = options.opened ?? new Set<string>()
  const flipped = options.flipped ?? new Set<string>()
  const homePaths = new Set(plan.home.map(file => file.path))

  // The files, the focus's first, then by the weight of what they hold.
  const files = new Map<string, { file: ScannedFile, members: string[], weight: number }>()
  const fileOf = (file: ScannedFile) => {
    let found = files.get(file.path)
    if (!found) files.set(file.path, found = { file, members: [], weight: 0 })
    return found
  }
  for (const file of plan.home) fileOf(file)
  for (const [id, weight] of plan.members) {
    const located = index.functions.get(id)
    if (!located || !keepFile(located.file)) continue
    const entry = fileOf(located.file)
    entry.members.push(id)
    entry.weight = Math.max(entry.weight, weight)
  }
  for (const file of plan.extra ?? []) if (keepFile(file)) fileOf(file)
  const ranked = [...files.values()].sort((a, b) => Number(homePaths.has(b.file.path)) - Number(homePaths.has(a.file.path)) || b.weight - a.weight || a.file.path.localeCompare(b.file.path))
  const kept = ranked.slice(0, maxCards)
  const over = ranked.slice(maxCards)

  // What each kept card shows.
  const cardOfPath = new Map<string, GraphCard>()
  const weightOf = new Map<string, number>()
  const visible = new Set<string>()
  const cards: GraphCard[] = []
  const allOf = (file: ScannedFile) => {
    const out: string[] = []
    walk(file.functions, (fn) => {
      if (keep(fn.id)) out.push(fn.id)
    })
    return out
  }
  const pending: { card: GraphCard, file: ScannedFile, members: string[] }[] = []
  const groups = new Map<string, GraphCard>()
  for (const { file, members, weight } of kept) {
    const id = fileCard(file.path)
    const home = homePaths.has(file.path)
    if (plan.groupOthers && !home) {
      // A row in its part's card: the lines from the focus end on the file.
      const groupId = partCard(file.part)
      let group = groups.get(file.part)
      if (!group) {
        group = { id: groupId, kind: 'part', name: file.part, rows: [], folded: 0, shut: flipped.has(groupId), open: false, home: false }
        groups.set(file.part, group)
        cards.push(group)
      }
      group.rows.push({ id: file.path, kind: 'file', depth: 0, context: false })
      weightOf.set(groupId, Math.max(weightOf.get(groupId) ?? 0, weight))
      for (const fn of members) visible.add(fn)
      cardOfPath.set(file.path, group)
      continue
    }
    const open = (home && !!plan.homeOpen) !== opened.has(id)
    const shut = (!home && !!plan.othersShut) !== flipped.has(id)
    const card: GraphCard = { id, kind: 'file', name: file.path, rows: [], folded: 0, shut, open, home }
    if (plan.columns) {
      const heaviest = [...members].sort((a, b) => (plan.members.get(b) ?? 0) - (plan.members.get(a) ?? 0))[0]
      card.column = home ? 0 : plan.columns.get(heaviest ?? '') ?? 1
    }
    const shown = open ? allOf(file) : members
    for (const fn of shown) visible.add(fn)
    for (const fn of members) visible.add(fn)
    cardOfPath.set(file.path, card)
    weightOf.set(id, weight)
    cards.push(card)
    pending.push({ card, file, members: shown })
  }

  // The edges between what is visible, as pairs of functions; worked out before the rows, which can depend on them.
  const pairs: { from: string, to: string, count: number, guessed: boolean }[] = []
  if (plan.edges) {
    for (const edge of plan.edges) {
      if (!visible.has(edge.from) || !visible.has(edge.to)) continue
      const call = index.callees.get(edge.from)?.find(item => item.to === edge.to)
      pairs.push({ ...edge, count: call?.count ?? 1, guessed: !!call?.guessed })
    }
  } else {
    for (const from of visible) {
      for (const edge of index.callees.get(from) ?? []) {
        if (edge.to !== from && visible.has(edge.to)) pairs.push({ from, to: edge.to, count: edge.count, guessed: !!edge.guessed })
      }
    }
  }
  const linked = new Set(pairs.flatMap(pair => [pair.from, pair.to]))

  for (const { card, file, members } of pending) {
    const wanted = new Set(card.home && plan.connectedOnly && !card.open ? members.filter(id => linked.has(id)) : members)
    // The functions the rows sit in (a method's class), as context.
    const context = new Set<string>()
    for (const id of wanted) {
      let parent = index.functions.get(id)?.parent ?? null
      while (parent && !wanted.has(parent)) {
        context.add(parent)
        parent = index.functions.get(parent)?.parent ?? null
      }
    }
    const depthOf = new Map<string, number>()
    walk(file.functions, (fn) => {
      if (!wanted.has(fn.id) && !context.has(fn.id)) return
      let depth = 0
      let parent = index.functions.get(fn.id)?.parent ?? null
      while (parent) {
        if (depthOf.has(parent)) {
          depth = depthOf.get(parent)! + 1
          break
        }
        parent = index.functions.get(parent)?.parent ?? null
      }
      depthOf.set(fn.id, depth)
      card.rows.push({ id: fn.id, kind: 'function', depth, context: !wanted.has(fn.id) })
    })
    card.folded = allOf(file).filter(id => !wanted.has(id)).length
  }
  for (const group of groups.values()) group.rows.sort((a, b) => a.id.localeCompare(b.id))

  // Too many rows: the lightest cards outside the focus are shut.
  let rows = cards.reduce((sum, card) => sum + (card.shut ? 0 : card.rows.length), 0)
  if (rows > maxRows) {
    for (const card of [...cards].filter(card => !card.home && !card.shut).sort((a, b) => weightOf.get(a.id)! - weightOf.get(b.id)!)) {
      if (rows <= maxRows) break
      card.shut = true
      rows -= card.rows.length
    }
  }

  // Where an edge ends: on its row, or on its card's header when the card is shut or the row is not drawn.
  const rowShown = new Set(cards.flatMap(card => (card.shut ? [] : card.rows.filter(row => !row.context).map(row => row.id))))
  const anchor = (id: string) => {
    const located = index.functions.get(id)
    const card = located ? cardOfPath.get(located.file.path) : undefined
    if (!card) return null
    if (card.kind === 'part') return card.shut ? card.id : located!.file.path
    return rowShown.has(id) ? id : card.id
  }
  const cardOfAnchor = new Map<string, GraphCard>()
  for (const card of cards) {
    cardOfAnchor.set(card.id, card)
    for (const row of card.rows) cardOfAnchor.set(row.id, card)
  }
  const edges = new Map<string, GraphEdge>()
  const inner = new Map<string, GraphEdge>()
  for (const pair of pairs) {
    const from = anchor(pair.from)
    const to = anchor(pair.to)
    if (!from || !to || from === to) continue
    const card = cardOfAnchor.get(from)
    // Two rows of one card: a mark on the rows, not a line looping out of the card.
    const same = card && card === cardOfAnchor.get(to)
    if (same && (from === card.id || to === card.id)) continue
    const into = same ? inner : edges
    const id = `${from}->${to}`
    const known = into.get(id)
    if (known) {
      known.count += pair.count
      known.guessed &&= pair.guessed
    } else {
      into.set(id, { id, from, to, kind: 'call', count: pair.count, guessed: pair.guessed, typeOnly: false, tone: 'plain' })
    }
  }
  if (options.imports) {
    for (const edge of importEdges(options.imports, path => cardOfPath.get(path)?.id ?? null)) edges.set(edge.id, edge)
  }

  // The focus's files on no line at all: one card listing them, not a grid of empty cards.
  if (plan.idleTogether) {
    const touched = new Set<string>()
    for (const edge of [...edges.values(), ...inner.values()]) {
      for (const end of [edge.from, edge.to]) {
        const card = cardOfAnchor.get(end)
        if (card) touched.add(card.id)
      }
    }
    const idle = cards.filter(card => card.kind === 'file' && card.home && !card.open && !card.rows.length && !touched.has(card.id))
    if (idle.length > 1) {
      const id = `idle:${plan.idleTogether}`
      const rows = idle.map(card => ({ id: card.name, kind: 'file' as const, depth: 0, context: false }))
      const gone = new Set(idle)
      cards.splice(0, cards.length, ...cards.filter(card => !gone.has(card)))
      cards.push({ id, kind: 'idle', name: plan.idleTogether, rows, folded: 0, shut: flipped.has(id), open: false, home: true })
    }
  }

  // What was left out, per part.
  const byPart = new Map<string, number>()
  for (const { file } of over) byPart.set(file.part, (byPart.get(file.part) ?? 0) + 1)
  const more = [...byPart].sort((a, b) => b[1] - a[1]).slice(0, MAX_MORE)
  for (const [part, count] of more) cards.push({ id: `more:${part}`, kind: 'more', name: part, rows: [], folded: 0, shut: true, open: false, home: false, count })
  return { cards, edges: [...edges.values()], inner: [...inner.values()], leftOut: over.length }
}

const TONES: Tone[] = ['plain', 'ok', 'known', 'new']
const worse = (a: Tone, b: Tone) => (TONES.indexOf(a) >= TONES.indexOf(b) ? a : b)
function toneOf(imported: { breaks?: string[], known?: boolean }, rules: boolean): Tone {
  if (!rules) return 'plain'
  if (!imported.breaks?.length) return 'ok'
  return imported.known ? 'known' : 'new'
}

/** The imports between the files shown, as edges between their anchors (`anchorOf` gives null for a file not shown). */
export function importEdges(options: ImportOptions, anchorOf: (path: string) => string | null): GraphEdge[] {
  const edges = new Map<string, GraphEdge>()
  const add = (from: string, to: string, typeOnly: boolean, tone: Tone) => {
    if (typeOnly && !options.typeOnly) return
    if (options.onlyBreaks && (tone === 'ok' || tone === 'plain')) return
    const a = anchorOf(from)
    const b = anchorOf(to)
    if (!a || !b || a === b) return
    const id = `${a}=>${b}`
    const known = edges.get(id)
    if (known) {
      known.count++
      known.typeOnly &&= typeOnly
      known.tone = worse(known.tone, tone)
    } else {
      edges.set(id, { id, from: a, to: b, kind: 'import', count: 1, guessed: false, typeOnly, tone })
    }
  }
  for (const group of options.between) for (const file of group.files) add(file.from, file.to, file.typeOnly, toneOf(file, options.rules))
  for (const item of options.inner ?? []) add(item.from, item.to, item.typeOnly, options.rules ? 'ok' : 'plain')
  return [...edges.values()]
}

/** The graph of a focus on a function, a file or a part. */
export function focusGraph(index: GraphIndex, focus: Focus, options: GraphOptions = {}): CodeGraph {
  const keep = (id: string) => (options.keep ? options.keep(id) : true) && !index.functions.get(id)?.fn.anonymous
  const keepFile = options.keepFile ?? (() => true)
  const depth = options.depth ?? 1
  const direction = options.direction ?? 'both'
  const named = (file: ScannedFile) => {
    const out: string[] = []
    walk(file.functions, (fn) => {
      if (keep(fn.id)) out.push(fn.id)
    })
    return out
  }
  if (focus.kind === 'function') {
    const located = index.functions.get(focus.id)
    if (!located) return { cards: [], edges: [], inner: [], leftOut: 0 }
    const columns = new Map<string, number>()
    return assemble(index, { members: reach(index, [focus.id], depth, direction, keep, columns), home: [located.file], columns }, options)
  }
  if (focus.kind === 'file') {
    const file = [...index.filesByPart.values()].flat().find(item => item.path === focus.path)
    if (!file) return { cards: [], edges: [], inner: [], leftOut: 0 }
    // With imports shown, the files it imports and the ones importing it are drawn too, shut.
    const extra: ScannedFile[] = []
    if (options.imports) {
      const byPath = new Map([...index.filesByPart.values()].flat().map(item => [item.path, item]))
      const paths = new Set<string>()
      for (const edge of importEdges(options.imports, path => path)) {
        if (edge.from === file.path) paths.add(edge.to)
        if (edge.to === file.path) paths.add(edge.from)
      }
      for (const path of paths) if (byPath.has(path)) extra.push(byPath.get(path)!)
    }
    const columns = new Map<string, number>()
    return assemble(index, { members: reach(index, named(file), depth, direction, keep, columns), home: [file], homeOpen: true, extra, othersShut: false, columns }, options)
  }
  if (focus.kind === 'part') {
    const files = (index.filesByPart.get(focus.name) ?? []).filter(keepFile)
    const seeds = files.flatMap(named)
    return assemble(index, { members: reach(index, seeds, depth, direction, keep), home: files, othersShut: true, connectedOnly: true, groupOthers: true, idleTogether: focus.name }, { ...options, maxCards: Math.max(options.maxCards ?? MAX_CARDS, Math.min(files.length + 12, 80)) })
  }
  return { cards: [], edges: [], inner: [], leftOut: 0 }
}

/** A path view as cards: the functions given, a row each in their file's card, and the edges given between them. */
export function pathGraph(index: GraphIndex, ids: string[], edges: { from: string, to: string }[], focus: string, options: GraphOptions = {}, steps?: Map<string, number>): CodeGraph {
  const members = new Map(ids.map((id, at) => [id, id === focus ? Infinity : ids.length - at]))
  const home = index.functions.get(focus)?.file
  return assemble(index, { members, home: home ? [home] : [], edges, columns: steps }, { ...options, maxCards: Math.max(options.maxCards ?? MAX_CARDS, 60) })
}

/** The project: a card per part, the files that import across parts as its rows, the imports between them. */
export function projectGraph(parts: Part[], filesByPart: Map<string, ScannedFile[]>, imports: ImportOptions, options: GraphOptions = {}): CodeGraph {
  const keepFile = options.keepFile ?? (() => true)
  const flipped = options.flipped ?? new Set<string>()
  const partOf = new Map<string, string>()
  const known = new Map<string, number>()
  for (const part of parts) {
    const files = (filesByPart.get(part.name) ?? []).filter(keepFile)
    known.set(part.name, files.length)
    for (const file of files) partOf.set(file.path, part.name)
  }
  // Which files take part, by the file-to-file edges with the filters applied.
  const fileEdges = importEdges({ ...imports, inner: null }, path => (partOf.has(path) ? path : null))
  const involved = new Set(fileEdges.flatMap(edge => [edge.from, edge.to]))
  const rowsOf = new Map<string, string[]>()
  for (const path of [...involved].sort()) {
    const part = partOf.get(path)!
    rowsOf.set(part, [...rowsOf.get(part) ?? [], path])
  }
  const total = [...rowsOf.values()].reduce((sum, list) => sum + list.length, 0)
  const startShut = total > PROJECT_OPEN_ROWS
  const cards: GraphCard[] = parts
    .filter(part => (known.get(part.name) ?? 0) > 0)
    .map((part) => {
      const id = partCard(part.name)
      const rows = (rowsOf.get(part.name) ?? []).map(path => ({ id: path, kind: 'file' as const, depth: 0, context: false }))
      const at = options.layerOrder?.indexOf(part.layer) ?? -1
      const column = options.layerOrder ? (at >= 0 ? at : options.layerOrder.length) : undefined
      return { id, kind: 'part' as const, name: part.name, rows, folded: known.get(part.name)! - rows.length, shut: rows.length > 0 && startShut !== flipped.has(id), open: false, home: false, column }
    })
  const shut = new Set(cards.filter(card => card.shut).map(card => card.name))
  const anchor = (path: string) => {
    const part = partOf.get(path)
    if (!part) return null
    return shut.has(part) ? partCard(part) : path
  }
  const edges = importEdges({ ...imports, inner: null }, anchor)
  // Two files of one part: an edge inside a card says nothing at this height.
  const sameCard = (edge: GraphEdge) => partOf.get(edge.from) === partOf.get(edge.to) && partOf.has(edge.from)
  return { cards, edges: edges.filter(edge => !sameCard(edge)), inner: [], leftOut: 0 }
}
