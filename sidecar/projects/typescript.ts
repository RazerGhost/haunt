import { existsSync, readFileSync } from 'node:fs'
import { basename, dirname, join } from 'node:path'
import type ts from 'typescript'
import type { Layer, PartImport, Rule } from '../../shared/types.ts'
import type { Judge } from '../imports.ts'
import { configuredJudge, configuredLocate, readConfig, type HauntConfig, type ConfiguredPart, type LayerConfig } from '../config.ts'
import { runScript, type ScriptArchitecture } from '../script.ts'
import { TYPESCRIPT_PROPER, isSource, languageOf } from '../languages.ts'
import { findSources, inRepository, isTestPath, type Located, type Project } from '../project.ts'
import { workspacePackages, type WorkspacePackage } from '../workspaces.ts'

/*
 * Any project, without configuration.
 *
 * - Files: the TypeScript files the root tsconfig.json includes, following its project references, and those each
 *   workspace package's tsconfig.json includes; and every file of the other kinds Haunt reads (JavaScript, Vue,
 *   Svelte, Astro, PHP, Rust, Lua) outside dependencies and builds. Without a tsconfig, TypeScript files the same way.
 * - Parts: the workspace packages, drawn in a band per folder they sit in (`apps`, `packages`…); files outside them by
 *   top folder. A project that is not a workspace: the folders in `src/` (Laravel: `app/`; `lib/`), or at the root.
 * - Layer rules only when the project has a haunt.config.json (config.ts): its part rules come first, and the files
 *   they do not match keep the parts above.
 */

const OTHER = 'other'
/** Folders whose subfolders are a project's parts when it is not a workspace. */
const SOURCE_FOLDERS = new Set(['src', 'app', 'lib'])

/** The project's name: package.json's, composer.json's or the root crate's in Cargo.toml, else the folder's. */
function packageName(root: string): string {
  for (const file of ['package.json', 'composer.json']) {
    try {
      const name = (JSON.parse(readFileSync(join(root, file), 'utf8')) as { name?: unknown }).name
      if (typeof name === 'string' && name) return name
    } catch { /* not there */ }
  }
  try {
    const crate = /^\s*\[package\][^[]*?^\s*name\s*=\s*"([^"]+)"/ms.exec(readFileSync(join(root, 'Cargo.toml'), 'utf8'))?.[1]
    if (crate) return crate
  } catch { /* not there */ }
  return basename(root)
}

interface Sources { files: string[], options: ts.CompilerOptions | null, ambient: string[] }

/** The files a tsconfig and the projects it references include, and the options of the first one that has files. */
function readConfigs(root: string, configs: string[], typescript: typeof ts): Sources {
  const files = new Set<string>()
  const ambient = new Set<string>()
  let options: ts.CompilerOptions | null = null
  const seen = new Set<string>()
  const host: ts.ParseConfigFileHost = { ...typescript.sys, onUnRecoverableConfigFileDiagnostic: () => {} }
  const read = (config: string) => {
    if (seen.has(config) || !existsSync(config)) return
    seen.add(config)
    const parsed = typescript.getParsedCommandLineOfConfigFile(config, {}, host)
    if (!parsed) return
    let added = 0
    for (const file of parsed.fileNames) {
      if (/\.d\.[cm]?ts$/.test(file)) ambient.add(file.split('\\').join('/'))
      const path = inRepository(root, file)
      if (path && languageOf(path) === 'typescript') {
        files.add(path)
        added++
      }
    }
    if (!options && added) options = parsed.options
    for (const reference of parsed.projectReferences ?? []) read(typescript.resolveProjectReferencePath(reference))
  }
  configs.forEach(read)
  return { files: [...files].sort(), options, ambient: [...ambient].sort() }
}

const DEFAULT_OPTIONS = (typescript: typeof ts): ts.CompilerOptions => ({
  target: typescript.ScriptTarget.ESNext,
  module: typescript.ModuleKind.ESNext,
  moduleResolution: typescript.ModuleResolutionKind.Bundler,
  jsx: typescript.JsxEmit.Preserve,
  strict: true
})

/** Where a file belongs: its workspace package, else a folder. */
function locateIn(packages: WorkspacePackage[]): (path: string) => Located {
  // The deepest package first, so a package inside another one wins.
  const byDepth = [...packages].sort((a, b) => b.folder.split('/').length - a.folder.split('/').length)
  return (path) => {
    const owner = byDepth.find(item => path.startsWith(`${item.folder}/`))
    if (owner) {
      const segments = owner.folder.split('/')
      return { part: owner.name, folder: owner.folder, layer: segments.length > 1 ? segments[0]! : 'packages' }
    }
    const segments = path.split('/')
    if (packages.length) {
      return segments.length > 1 ? { part: segments[0]!, folder: segments[0]!, layer: OTHER } : { part: '(root)', folder: '', layer: OTHER }
    }
    // Not a workspace: the folders in the source folder (src/, Laravel's app/, lib/), or at the root.
    if (SOURCE_FOLDERS.has(segments[0]!) && segments.length > 2) return { part: segments[1]!, folder: `${segments[0]}/${segments[1]}`, layer: 'parts' }
    if (segments.length > 1) return { part: segments[0]!, folder: segments[0]!, layer: 'parts' }
    return { part: '(root)', folder: '', layer: 'parts' }
  }
}

export async function typescriptProject(root: string, typescript: typeof ts, trusted: boolean): Promise<Project> {
  const packages = workspacePackages(root)
  const configs = [join(root, 'tsconfig.json'), ...packages.map(item => join(root, item.folder, 'tsconfig.json'))]
  const found = readConfigs(root, configs, typescript)
  // TypeScript files proper from the tsconfigs (their `include` decides); JavaScript, Vue, Svelte and Astro files and
  // the other languages wherever they are. Without a tsconfig, every file of a language Haunt reads.
  const elsewhere = findSources(root, '', path => isSource(path) && (!found.files.length || !TYPESCRIPT_PROPER.test(path)))
  const files = [...new Set([...found.files, ...elsewhere])].sort()

  // The project's own layer map, if it has one (haunt.config.json, or the script it names); its rules come before the
  // zero-config parts.
  let rules: Rules | null = null
  let architectureError: string | null = null
  let restartOn: string[] = []
  try {
    const config = readConfig(root)
    if (config?.architecture) {
      restartOn = [dirname(config.architecture.script)]
      if (trusted) rules = scriptRules(await runScript(root, config.architecture.script, files), files)
    } else if (config) {
      rules = configRules(config)
    }
  } catch (error) {
    architectureError = error instanceof Error ? error.message : String(error)
  }
  const zeroConfig = locateIn(packages)
  // Where each file is, worked out once: it is asked for every file and every import, several times a scan.
  const located = new Map<string, Located>()
  const locate = (path: string): Located => {
    const known = located.get(path)
    if (known) return known
    const hit = rules?.locate(path)
    const found = hit ? { part: hit.part, folder: hit.folder, layer: hit.layer } : zeroConfig(path)
    located.set(path, found)
    return found
  }
  const described = new Map<string, { role: string, uses: string[] }>()
  for (const path of files) {
    const hit = rules?.locate(path)
    if (hit && !described.has(hit.part)) described.set(hit.part, { role: hit.role, uses: hit.uses })
  }

  const ruledIds = new Set(rules?.layers.map(layer => layer.id))
  const bands = [...new Set(files.map(path => locate(path).layer))].filter(id => !ruledIds.has(id))
  const order = (id: string) => id === 'apps' ? 0 : id === OTHER ? 2 : 1
  const layers: Layer[] = [
    ...(rules?.layers ?? []).map(layer => ({ id: layer.id, label: layer.label ?? layer.id, note: layer.note ?? '', ruled: true })),
    ...bands
      .sort((a, b) => order(a) - order(b) || a.localeCompare(b))
      .map(id => ({ id, label: id, note: id === OTHER ? 'files outside the workspace packages' : '' }))
  ]

  // The top folders that hold scanned files; files at the root are seen by the root's own watcher.
  const top = [...new Set(files.filter(path => path.includes('/')).map(path => path.split('/')[0]!))]
  return {
    kind: 'typescript',
    name: packageName(root),
    files,
    options: { ...DEFAULT_OPTIONS(typescript), ...found.options ?? {} },
    ambient: found.ambient,
    layers,
    locate,
    isTest: isTestPath,
    describe: part => described.get(part) ?? { role: '', uses: [] },
    rules: rules && described.size ? rules.kind : null,
    ownImports: rules?.ownImports ?? null,
    judge: rules?.judge ?? null,
    architectureError,
    // The script's folder too: a change there restarts the scanner, so the script's new code runs.
    watch: [...new Set([...top, ...restartOn.filter(folder => folder !== '.')])],
    restartOn: restartOn.filter(folder => folder !== '.')
  }
}

/** The rules of a project, from its haunt.config.json or its architecture script. */
interface Rules {
  kind: 'config' | 'script'
  layers: LayerConfig[]
  locate: (path: string) => ConfiguredPart | null
  judge: Judge | null
  ownImports: { imports: PartImport[], cycles: string[][] } | null
}

function configRules(config: HauntConfig): Rules {
  // Each rule's pattern runs at every folder depth: once per file, then remembered (the judge locates both ends of
  // every import too).
  const configured = configuredLocate(config)
  const located = new Map<string, ConfiguredPart | null>()
  const locate = (path: string): ConfiguredPart | null => {
    if (!located.has(path)) located.set(path, configured(path))
    return located.get(path)!
  }
  return { kind: 'config', layers: config.layers, locate, judge: configuredJudge(config, locate), ownImports: null }
}

/** The deepest folder all of a part's files share. */
function commonFolder(paths: string[]): string {
  const split = paths.map(path => path.split('/').slice(0, -1))
  const first = split[0] ?? []
  let depth = 0
  while (depth < first.length && split.every(segments => segments[depth] === first[depth])) depth++
  return first.slice(0, depth).join('/')
}

function scriptRules(script: ScriptArchitecture, files: string[]): Rules {
  const parts = new Map(script.parts.map(part => [part.name, part]))
  const partOf = (path: string): string | null => {
    const name = script.partOf(path)
    return typeof name === 'string' && parts.has(name) ? name : null
  }
  const filesOf = new Map<string, string[]>()
  for (const path of files) {
    const name = partOf(path)
    if (!name) continue
    const list = filesOf.get(name)
    if (list) list.push(path)
    else filesOf.set(name, [path])
  }
  // Each part's folder, worked out once: locate runs for every file and every import, several times.
  const folderOf = new Map<string, string>()
  const locate = (path: string): ConfiguredPart | null => {
    const name = partOf(path)
    if (!name) return null
    const part = parts.get(name)!
    let folder = part.folder ?? folderOf.get(name)
    if (folder === undefined) {
      folder = commonFolder(filesOf.get(name) ?? [path])
      if (filesOf.has(name)) folderOf.set(name, folder)
    }
    return { part: name, folder, layer: part.layer, uses: part.uses ?? [], role: part.role ?? '' }
  }
  let ownImports: Rules['ownImports'] = null
  if (script.imports) {
    // The script's own judged imports, grouped by the parts it names.
    const groups = new Map<string, PartImport>()
    for (const edge of script.imports) {
      const from = partOf(edge.from), to = partOf(edge.to)
      if (!from || !to || from === to) continue
      const key = `${from}\n${to}`
      const group = groups.get(key) ?? { from, to, files: [] }
      group.files.push({ from: edge.from, to: edge.to, typeOnly: !!edge.typeOnly, breaks: edge.breaks ?? [], known: !!edge.known })
      groups.set(key, group)
    }
    ownImports = { imports: [...groups.values()], cycles: script.cycles ?? [] }
  }
  const judge: Judge | null = script.judge
    ? (from, to) => {
        const breaks = script.judge!(from, to)
        return Array.isArray(breaks) ? breaks.filter((rule): rule is Rule => rule === 'layer' || rule === 'door') : []
      }
    : null
  return { kind: 'script', layers: script.layers, locate, judge, ownImports }
}
