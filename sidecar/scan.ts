import { readFileSync, statSync } from 'node:fs'
import { join, sep } from 'node:path'
import ts from 'typescript'
import type { CallEdge, FunctionNode, InnerImport, Part, Scan, ScannedFile } from '../shared/types.ts'
import { callGraph, callGraphInTurns, type CallGraph, type Uses } from './calls.ts'
import { sourceFileOf, viewSource } from './frameworks.ts'
import { fileFunctions } from './functions.ts'
import { importGraph, importsIn, innerImports, partImports, pathKey, runtimeCycles, type FileEdge } from './imports.ts'
import { FRAMEWORK, languageOf } from './languages.ts'
import { detectProject, type Project } from './project.ts'
import { findRoutes, type Route } from './routes.ts'
import { cargoManifests } from './workspaces.ts'

/*
 * One scan of a repository: what the project says to read (project.ts), the functions in every file, and then the
 * call graph, which needs the type checker. The TypeScript family (TypeScript, JavaScript, and Vue, Svelte and Astro
 * files through their TypeScript views, frameworks.ts) goes through the TypeScript compiler. PHP, Rust and Lua are read
 * by the app (`readOthers`: their functions, imports and calls matched by name); without it, they are left out. Parsed
 * files are kept between scans (by modification time), so a rescan only parses what changed.
 */

/** What the app reads of the PHP, Rust and Lua files: by path, and the calls between them, matched by name. */
export interface OtherFiles {
  files: Record<string, { tree: FunctionNode[], imports: { to: string, typeOnly: boolean }[], lines: number }>
  calls: CallEdge[]
}

export type ReadOthers = (files: { path: string, test: boolean }[], cargoManifests: string[]) => Promise<OtherFiles>

/** An import found in a version of a file, resolved to a scanned file and judged by the project's rules. */
export interface VersionImport {
  to: string
  typeOnly: boolean
  fromPart: string
  toPart: string
  /** The rules it breaks; empty when it breaks none, or when nothing judges imports (`judged` is false). */
  breaks: string[]
}

export interface Scanner {
  /** The structure: everything but the call graph (`calls` is null). */
  structure: () => Promise<Scan>
  /** The call graph for the last structure, and how each TypeScript-family function is used (calls.ts `Uses`). */
  calls: () => { calls: CallEdge[], uses: Uses }
  /** The same in turns, answering questions meanwhile; null when `stop` said a newer scan came first. */
  callsInTurns: (stop: () => boolean) => Promise<CallGraph | null>
  /**
   * The imports of other versions of files (by key: a path and its text), resolved as the scan resolves them, against
   * the files as they are now, and judged by the project's rules (`judged`: whether anything judges them).
   */
  imports: (files: { key: string, path: string, text: string }[]) => { judged: boolean, files: Record<string, VersionImport[]> }
  /** The HTTP routes in the TypeScript family's files of the last structure (routes.ts). */
  routes: () => Route[]
  /** The settings files the last structure's call graph also depends on: each part's tsconfig.json and package.json, and the declaration files the tsconfigs list (a framework's generated globals). */
  settings: () => string[]
}

interface Parsed { mtime: number, sf: ts.SourceFile }

function compileOptions(project: Project): ts.CompilerOptions {
  return { ...project.options, allowJs: true, checkJs: false, noEmit: true, skipLibCheck: true, types: [], allowImportingTsExtensions: true }
}

/** Scans `root`; call `structure()` first, then `calls()` for the same files. `trusted`: the folder's own code may run. */
export function scanner(root: string, { trusted = false, readOthers }: { trusted?: boolean, readOthers?: ReadOthers } = {}): Scanner {
  // Parsed files by absolute path (forward slashes): the scanned ones, and the declaration files of dependencies the
  // checker reads. A Vue, Svelte or Astro file is parsed as its TypeScript view, under the view's name.
  const parsed = new Map<string, Parsed>()
  /** A Vue, Svelte or Astro file's whole text, by its view's name: its template can use what its script declares. */
  const whole = new Map<string, string>()
  const parse = (file: string): ts.SourceFile | undefined => {
    const stats = statSync(file, { throwIfNoEntry: false })
    if (!stats) return undefined
    const mtime = stats.mtimeMs
    const hit = parsed.get(file)
    if (hit && hit.mtime === mtime) return hit.sf
    const text = readFileSync(file, 'utf8')
    const sf = sourceFileOf(file, text)
    if (FRAMEWORK.test(file)) whole.set(sf.fileName, text)
    parsed.set(file, { mtime, sf })
    return sf
  }
  /** A compiler host that knows the views: `Card.svelte.ts` exists when `Card.svelte` does, and reads as its view. */
  const host = (options: ts.CompilerOptions): ts.CompilerHost => {
    const base = ts.createCompilerHost(options, true)
    const fileExists = base.fileExists.bind(base)
    const readFile = base.readFile.bind(base)
    base.fileExists = (name) => {
      const source = viewSource(name)
      return source ? parse(source)?.fileName === name : fileExists(name)
    }
    base.readFile = (name) => {
      const source = viewSource(name)
      return source ? parse(source)?.text : readFile(name)
    }
    base.getSourceFile = name => parse(viewSource(name) ?? name)
    return base
  }

  let project: Project | null = null
  let scanned: string[] = []
  /** The scanned files' repository paths, by their parsed names (lowercased where the file system ignores case). */
  let byName = new Map<string, string>()
  let functions = new Map<string, Map<ts.Node, FunctionNode>>()
  /** The calls of PHP, Rust and Lua, matched by name when the structure was read. */
  let guessed: CallEdge[] = []
  /** The last call graph's program, for the routes; made again when the structure changed since. */
  let program: ts.Program | null = null
  const makeProgram = (current: Project): ts.Program => {
    const options = compileOptions(current)
    program = ts.createProgram({ rootNames: [...scanned, ...current.ambient], options, host: host(options), oldProgram: program ?? undefined })
    // What the program no longer reads (a deleted file, another branch's dependencies) is not kept.
    const live = new Set(program.getSourceFiles().map(sf => sf.fileName))
    for (const [file, { sf }] of parsed) if (!live.has(sf.fileName)) parsed.delete(file)
    return program
  }
  /**
   * A call graph finished: framework templates' mentions as uses, and the guessed calls of the other languages. The
   * program reads the files parsed above (so the function tree's nodes are the checker's) and keeps the declaration
   * files of dependencies between scans, the tsconfigs' too (a framework's generated globals resolve to what they
   * name); only the checking is done again.
   */
  const complete = (program: ts.Program, graph: CallGraph): CallGraph => {
    // Framework templates use names the type checker does not see: a word a file's whole text has more often than its
    // script is one its template mentions. A function of that name is used there (from its own file, or another).
    const mentioned = new Map<string, Set<string>>()
    const words = (text: string) => {
      const counts = new Map<string, number>()
      for (const [word] of text.matchAll(/[A-Za-z_$][\w$]*/g)) counts.set(word, (counts.get(word) ?? 0) + 1)
      return counts
    }
    for (const [name, text] of whole) {
      const script = words(program.getSourceFile(name)?.text ?? '')
      for (const [word, count] of words(text)) {
        if (count > (script.get(word) ?? 0)) (mentioned.get(word) ?? mentioned.set(word, new Set()).get(word)!).add(name)
      }
    }
    for (const [name, nodes] of functions) {
      for (const item of nodes.values()) {
        const files = mentioned.get(item.name)
        if (!files || item.anonymous || graph.uses[item.id] === 'outside') continue
        graph.uses[item.id] = [...files].some(file => file !== name) ? 'outside' : 'inside'
      }
    }
    return { calls: [...graph.calls, ...guessed], uses: graph.uses }
  }
  return {
    async structure() {
      const started = performance.now()
      project = await detectProject(root, ts, trusted)
      program = null
      functions = new Map()
      byName = new Map()
      const files: ScannedFile[] = []
      const parts = new Map<string, Part>()
      const sources = new Map<string, ts.SourceFile>()
      const others: { path: string, test: boolean }[] = []
      for (const path of project.files) {
        const { part, folder, layer } = project.locate(path)
        if (!parts.has(part)) parts.set(part, { name: part, layer, folder, ...project.describe(part) })
        const language = languageOf(path)
        if (!language) continue
        const test = project.isTest(path)
        if (language !== 'typescript') {
          others.push({ path, test })
          continue
        }
        const sf = parse(join(root, path).split(sep).join('/'))
        if (!sf) continue
        if (!test) sources.set(path, sf)
        const found = fileFunctions(sf, path, test)
        functions.set(sf.fileName, found.byNode)
        byName.set(pathKey(sf.fileName), path)
        files.push({ path, part, test, lines: sf.getLineAndCharacterOfPosition(sf.getEnd()).line + 1, functions: found.tree })
      }
      scanned = [...functions.keys()]
      // A Vue, Svelte or Astro file deleted or renamed since: its old text would still count as template mentions.
      for (const name of [...whole.keys()]) if (!functions.has(name)) whole.delete(name)

      // PHP, Rust and Lua, read by the app: their functions, imports, and calls matched by name.
      const readings = others.length && readOthers ? await readOthers(others, cargoManifests(root)) : { files: {}, calls: [] }
      const otherEdges: FileEdge[] = []
      for (const { path, test } of others) {
        const reading = readings.files[path]
        if (!reading) continue
        files.push({ path, part: project.locate(path).part, test, lines: reading.lines, functions: reading.tree })
        if (!test) for (const to of reading.imports) otherEdges.push({ from: path, to: to.to, typeOnly: to.typeOnly })
      }
      files.sort((a, b) => a.path.localeCompare(b.path))
      guessed = readings.calls

      // The imports between parts: the script's own, or Haunt's graph of the non-test files (with the ones within each
      // part too, which only Haunt's graph has).
      let imports = project.ownImports
      let inner: InnerImport[] | null = null
      if (!imports) {
        const options = compileOptions(project)
        const edges = [...importGraph(root, sources, options, ts, host(options)), ...otherEdges]
        const located = project.locate
        const nonTest = [...sources.keys(), ...others.filter(other => !other.test).map(other => other.path)]
        imports = { imports: partImports(edges, path => located(path).part, project.judge), cycles: runtimeCycles(nonTest, edges) }
        inner = innerImports(edges, path => located(path).part)
      }
      return {
        root,
        project: { kind: project.kind, name: project.name },
        watch: project.watch,
        restartOn: project.restartOn,
        trusted,
        scannedAt: new Date().toISOString(),
        ms: { structure: Math.round(performance.now() - started), calls: null },
        layers: project.layers.filter(layer => [...parts.values()].some(part => part.layer === layer.id)),
        parts: [...parts.values()],
        files,
        calls: null,
        imports: imports.imports,
        innerImports: inner,
        cycles: imports.cycles,
        rules: project.rules,
        architectureError: project.architectureError
      }
    },
    calls() {
      if (!project) throw new Error('Scan the structure first.')
      const program = makeProgram(project)
      return complete(program, callGraph(program, functions))
    },
    async callsInTurns(stop) {
      if (!project) throw new Error('Scan the structure first.')
      const program = makeProgram(project)
      const graph = await callGraphInTurns(program, functions, stop)
      return graph && complete(program, graph)
    },
    settings() {
      if (!project) return []
      const folders = [...new Set(project.files.map(path => project!.locate(path).folder))].filter(folder => folder && folder !== '.')
      return [...folders.flatMap(folder => [`${folder}/tsconfig.json`, `${folder}/package.json`]), ...project.ambient]
    },
    routes() {
      if (!project) throw new Error('Scan the structure first.')
      return findRoutes(program ?? makeProgram(project), functions, name => byName.get(pathKey(name)))
    },
    imports(files) {
      if (!project) throw new Error('Scan the structure first.')
      const current = project
      const options = compileOptions(current)
      const resolver = host(options)
      const cache = ts.createModuleResolutionCache(root, name => ts.sys.useCaseSensitiveFileNames ? name : name.toLowerCase(), options)
      const out: Record<string, VersionImport[]> = {}
      for (const { key, path, text } of files) {
        const file = join(root, path).split(sep).join('/')
        // The version's own text, under the file's name, so its relative imports resolve from where it is.
        const sf = sourceFileOf(file, text)
        const fromPart = current.locate(path).part
        const found = new Map<string, VersionImport>()
        for (const { specifier, typeOnly } of importsIn(sf, ts)) {
          const resolved = ts.resolveModuleName(specifier, sf.fileName, options, resolver, cache).resolvedModule
          const to = resolved && byName.get(pathKey(resolved.resolvedFileName))
          if (!to || to === path) continue
          const known = found.get(to)
          if (known) {
            known.typeOnly &&= typeOnly
            continue
          }
          const toPart = current.locate(to).part
          const breaks = current.judge && fromPart !== toPart ? current.judge({ part: fromPart, path }, { part: toPart, path: to }) : []
          found.set(to, { to, typeOnly, fromPart, toPart, breaks })
        }
        out[key] = [...found.values()]
      }
      return { judged: !!current.judge, files: out }
    }
  }
}


