<script setup lang="ts">
import type { ThemedToken } from 'shiki/core'
import type { DiffHunk, DiffLine, FileDiffView } from '~~/shared/types'
import { changedSpans, markSpans, type Span } from '~~/shared/wordDiff'

/*
 * One file's diff between two commits: unified or side by side, with syntax colours, the words that changed marked
 * within a changed line, and the function a hunk is in (as git names it) above it. "Whole file" shows every line.
 */
const props = defineProps<{ from: string | null, to: string, path: string, oldPath?: string }>()
const webs = useWeb()
const { diffWrap, tabWidth, ignoreSpace } = usePreferences()
const copyPath = () => navigator.clipboard?.writeText(props.path)
const { structure: scan, select } = useCode()
/** The scan's function a hunk's header names (the innermost one around the hunk when several match), to open it. */
function functionOf(section: string, at: number): string | null {
  const file = scan.value?.files.find(item => item.path === props.path)
  if (!file || !section) return null
  const found: { id: string, name: string, line: number, endLine: number }[] = []
  const walk = (items: typeof file.functions) => items.forEach((fn) => {
    if (!fn.anonymous && new RegExp(`\\b${fn.name.replace(/[$()*+.?[\\\]^{|}]/g, '\\$&')}\\b`).test(section)) found.push(fn)
    walk(fn.children)
  })
  walk(file.functions)
  const around = found.filter(fn => fn.line <= at && at <= fn.endLine).sort((a, b) => (a.endLine - a.line) - (b.endLine - b.line))
  return (around[0] ?? found[0])?.id ?? null
}

const diff = shallowRef<FileDiffView | null>(null)
const problem = ref('')
const whole = ref(false)
const split = usePersisted('diff-split', false, value => typeof value === 'boolean')
/** Tokens per hunk: the old side's lines (context and removed), the new side's (context and added). */
const tokens = shallowRef<{ old: ThemedToken[][] | null, new: ThemedToken[][] | null }[]>([])
const { repo } = useRepositories()
const ask = latest(repo)
const { loading } = ask

async function load(): Promise<void> {
  try {
    const view = await ask.run(at => haunt.diff(at, props.from, props.to, props.path, props.oldPath ?? null, whole.value ? 1_000_000 : 3, ignoreSpace.value))
    if (view === undefined) return
    diff.value = view
    tokens.value = view.hunks.map(() => ({ old: null, new: null }))
    problem.value = ''
    const coloured = await Promise.all(view.hunks.map(async hunk => ({
      old: await highlight(props.oldPath ?? props.path, hunk.lines.filter(line => line.kind !== 'add').map(line => line.text)),
      new: await highlight(props.path, hunk.lines.filter(line => line.kind !== 'del').map(line => line.text))
    })))
    // The colours only while this diff is still the one shown.
    if (diff.value === view) tokens.value = coloured
  } catch (error) {
    problem.value = errorMessage(error, 'The diff could not be read.')
  }
}
watch(() => [props.from, props.to, props.path, props.oldPath, whole.value, ignoreSpace.value], () => void load(), { immediate: true })

interface Piece { content: string, htmlStyle?: Record<string, string>, marked: boolean }
interface Shown { kind: DiffLine['kind'], old: number | null, new: number | null, pieces: Piece[] }

/** Each hunk's lines with their colours and marks. */
const shown = computed(() => (diff.value?.hunks ?? []).map((hunk, index) => shownHunk(hunk, tokens.value[index])))

function shownHunk(hunk: DiffHunk, coloured?: { old: ThemedToken[][] | null, new: ThemedToken[][] | null }): { hunk: DiffHunk, lines: Shown[] } {
  // Pair each run of removed lines with the run of added lines after it, line by line, for the word marks.
  const spans = new Map<number, Span[]>()
  for (let at = 0; at < hunk.lines.length;) {
    if (hunk.lines[at]!.kind !== 'del') {
      at++
      continue
    }
    const dels: number[] = []
    while (hunk.lines[at]?.kind === 'del') dels.push(at++)
    const adds: number[] = []
    while (hunk.lines[at]?.kind === 'add') adds.push(at++)
    for (let k = 0; k < Math.min(dels.length, adds.length); k++) {
      const marks = changedSpans(hunk.lines[dels[k]!]!.text, hunk.lines[adds[k]!]!.text)
      spans.set(dels[k]!, marks.before)
      spans.set(adds[k]!, marks.after)
    }
  }
  let oldAt = 0
  let newAt = 0
  const lines = hunk.lines.map((line, index): Shown => {
    const own = line.kind === 'add' ? coloured?.new?.[newAt] : coloured?.old?.[oldAt]
    // A context line is on both sides; count it on both.
    if (line.kind !== 'add') oldAt++
    if (line.kind !== 'del') newAt++
    const pieces = own?.length ? own.map(token => ({ content: token.content, htmlStyle: token.htmlStyle as Record<string, string> | undefined })) : [{ content: line.text }]
    return { kind: line.kind, old: line.old, new: line.new, pieces: markSpans(pieces, spans.get(index) ?? []) }
  })
  return { hunk, lines }
}

/** Side by side: context on both sides; within a change, removed lines left and added lines right, row by row. */
const pairs = computed(() => shown.value.map(({ hunk, lines }) => {
  const rows: { left: Shown | null, right: Shown | null }[] = []
  for (let at = 0; at < lines.length;) {
    const line = lines[at]!
    if (line.kind === 'context') {
      rows.push({ left: line, right: line })
      at++
      continue
    }
    const dels: Shown[] = []
    while (lines[at]?.kind === 'del') dels.push(lines[at++]!)
    const adds: Shown[] = []
    while (lines[at]?.kind === 'add') adds.push(lines[at++]!)
    for (let k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ left: dels[k] ?? null, right: adds[k] ?? null })
  }
  return { hunk, rows }
}))

const BACKGROUND = { add: 'bg-success/10', del: 'bg-error/10', context: '' } as const
const MARK = { add: 'bg-success/30', del: 'bg-error/30', context: '' } as const
const SIGN = { add: '+', del: '−', context: ' ' } as const
</script>

<template>
  <div class="flex min-h-0 flex-col">
    <div class="flex items-center gap-3 border-b border-default px-3 py-1.5 text-xs">
      <span class="min-w-0 flex-1 truncate font-mono text-highlighted" :title="props.oldPath && props.oldPath !== props.path ? `${props.oldPath} → ${props.path}` : props.path">
        <template v-if="props.oldPath && props.oldPath !== props.path"><span class="text-muted">{{ props.oldPath }} → </span></template>{{ props.path }}
      </span>
      <UIcon v-if="loading" name="i-lucide-loader-circle" class="size-3.5 animate-spin text-muted" />
      <UButton label="History" icon="i-lucide-history" size="xs" color="neutral" variant="ghost" :to="{ path: '/history', query: { path: props.path, at: props.to } }" />
      <UButton label="Blame" icon="i-lucide-user-round-search" size="xs" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path: props.path, at: props.to } }" />
      <UButton icon="i-lucide-copy" size="xs" color="neutral" variant="ghost" title="Copy the path" aria-label="Copy the path" @click="copyPath" />
      <UButton v-if="webs.label.value" icon="i-lucide-external-link" size="xs" color="neutral" variant="ghost" :title="webs.label.value" :aria-label="webs.label.value" @click="webs.openFile(props.to, props.path)" />
      <UButton v-if="webs.copyLabel.value" icon="i-lucide-link" size="xs" color="neutral" variant="ghost" :title="webs.copyLabel.value" :aria-label="webs.copyLabel.value" @click="webs.copyFile(props.to, props.path)" />
      <USwitch v-model="whole" label="Whole file" size="xs" />
      <UFieldGroup size="xs">
        <UButton label="Unified" :color="split ? 'neutral' : 'primary'" :variant="split ? 'outline' : 'solid'" @click="split = false" />
        <UButton label="Side by side" :color="split ? 'primary' : 'neutral'" :variant="split ? 'solid' : 'outline'" @click="split = true" />
      </UFieldGroup>
    </div>

    <p v-if="problem" class="p-3 text-sm text-error">{{ problem }}</p>
    <p v-else-if="diff?.binary" class="p-3 text-sm text-muted">A binary file: no lines to compare.</p>
    <p v-else-if="diff && !diff.hunks.length" class="p-3 text-sm text-muted">No lines changed (the file was renamed or its mode changed).</p>

    <div v-else-if="diff" class="min-h-0 flex-1 overflow-auto font-mono text-xs leading-[1.6]">
      <template v-if="!split">
        <table class="w-full border-collapse">
          <tbody v-for="({ hunk, lines }, index) in shown" :key="index">
            <tr class="bg-elevated/60 text-muted">
              <td colspan="4" class="px-3 py-0.5 text-label">
                <span class="text-dimmed">@@ −{{ hunk.oldStart }} +{{ hunk.newStart }} @@</span>
                <button v-if="hunk.section && functionOf(hunk.section, hunk.newStart)" type="button" class="ml-2 text-default hover:underline" title="Open this function in the panel" @click="select(functionOf(hunk.section, hunk.newStart))">{{ hunk.section }}</button>
                <span v-else-if="hunk.section" class="ml-2 text-default">{{ hunk.section }}</span>
              </td>
            </tr>
            <tr v-for="(line, row) in lines" :key="row" :class="BACKGROUND[line.kind]">
              <td class="w-12 px-2 text-right text-dimmed select-none">{{ line.old ?? '' }}</td>
              <td class="w-12 px-2 text-right text-dimmed select-none">{{ line.new ?? '' }}</td>
              <td class="w-4 text-center text-muted select-none">{{ SIGN[line.kind] }}</td>
              <td class="pr-4" :class="diffWrap ? 'break-all whitespace-pre-wrap' : 'whitespace-pre'" :style="{ tabSize: tabWidth }"><span v-for="(piece, at) in line.pieces" :key="at" class="code-token" :class="piece.marked ? MARK[line.kind] : ''" :style="piece.htmlStyle">{{ piece.content }}</span></td>
            </tr>
          </tbody>
        </table>
      </template>
      <template v-else>
        <table class="w-full table-fixed border-collapse">
          <tbody v-for="({ hunk, rows }, index) in pairs" :key="index">
            <tr class="bg-elevated/60 text-muted">
              <td colspan="4" class="px-3 py-0.5 text-label">
                <span class="text-dimmed">@@ −{{ hunk.oldStart }} +{{ hunk.newStart }} @@</span>
                <button v-if="hunk.section && functionOf(hunk.section, hunk.newStart)" type="button" class="ml-2 text-default hover:underline" title="Open this function in the panel" @click="select(functionOf(hunk.section, hunk.newStart))">{{ hunk.section }}</button>
                <span v-else-if="hunk.section" class="ml-2 text-default">{{ hunk.section }}</span>
              </td>
            </tr>
            <tr v-for="(pair, row) in rows" :key="row">
              <td class="w-12 px-2 text-right align-top text-dimmed select-none" :class="pair.left ? BACKGROUND[pair.left.kind] : 'bg-elevated/30'">{{ pair.left?.old ?? '' }}</td>
              <td class="overflow-hidden border-r border-default pr-2 align-top" :class="[pair.left ? BACKGROUND[pair.left.kind] : 'bg-elevated/30', diffWrap ? 'break-all whitespace-pre-wrap' : 'whitespace-pre']" :style="{ tabSize: tabWidth }"><span v-for="(piece, at) in pair.left?.pieces ?? []" :key="at" class="code-token" :class="piece.marked ? MARK.del : ''" :style="piece.htmlStyle">{{ piece.content }}</span></td>
              <td class="w-12 px-2 text-right align-top text-dimmed select-none" :class="pair.right ? BACKGROUND[pair.right.kind] : 'bg-elevated/30'">{{ pair.right?.new ?? '' }}</td>
              <td class="overflow-hidden pr-2 align-top" :class="[pair.right ? BACKGROUND[pair.right.kind] : 'bg-elevated/30', diffWrap ? 'break-all whitespace-pre-wrap' : 'whitespace-pre']" :style="{ tabSize: tabWidth }"><span v-for="(piece, at) in pair.right?.pieces ?? []" :key="at" class="code-token" :class="piece.marked ? MARK.add : ''" :style="piece.htmlStyle">{{ piece.content }}</span></td>
            </tr>
          </tbody>
        </table>
      </template>
    </div>
  </div>
</template>
