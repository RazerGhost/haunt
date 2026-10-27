<script setup lang="ts">
import type { ThemedToken } from 'shiki/core'
import type { DiffLine, FileDiffView, LineRef } from '~/bindings'

/*
 * One file's local diff, on the side it was picked from: unstaged (the index against the working tree, or a whole new
 * file) or staged (the last commit against the index). Click a changed line to pick it, shift-click to pick a range;
 * the picked lines are staged, unstaged or discarded on their own, the rest stays as it is. Without a pick, the
 * buttons act on the whole file. A new file's lines, and a binary file's, are taken whole. The functions the changes
 * are in show above the diff: click one to pick all of its changed lines, so a function is staged, unstaged or
 * discarded at once.
 */
const props = defineProps<{ path: string, oldPath: string, staged: boolean, untracked: boolean, conflicted: boolean }>()
const emit = defineEmits<{ discard: [what: { lines: LineRef[] } | { file: true }] }>()
const { repo } = useRepositories()
const { stage, unstage, lines: writeLines, busy } = useLocalChanges()
const { gitChanged } = useRepositories()
const changed = gitChanged('head', 'index')
const { structure: scan } = useCode()
// Local diffs keep whitespace (lines are staged from them exactly); wrapping and a tab's width apply.
const { diffWrap, tabWidth } = usePreferences()

const diff = shallowRef<FileDiffView | null>(null)
const problem = ref('')
const tokens = shallowRef<{ old: ThemedToken[][] | null, new: ThemedToken[][] | null }[]>([])
/** The picked lines, by side and number ("o12": removed line 12, "n5": added line 5). */
const picked = ref(new Set<string>())
const lastPicked = ref<number | null>(null)
const ask = latest(repo)

async function load(): Promise<void> {
  try {
    const view = await ask.run(at => haunt.localDiff(at, props.path, props.staged ? props.oldPath : null, props.staged))
    if (view === undefined) return
    diff.value = view
    problem.value = ''
    tokens.value = view.hunks.map(() => ({ old: null, new: null }))
    const coloured = await Promise.all(view.hunks.map(async hunk => ({
      old: await highlight(props.oldPath, hunk.lines.filter(line => line.kind !== 'add').map(line => line.text)),
      new: await highlight(props.path, hunk.lines.filter(line => line.kind !== 'del').map(line => line.text))
    })))
    if (diff.value === view) tokens.value = coloured
  } catch (error) {
    diff.value = null
    problem.value = errorMessage(error, t('LocalDiff.notRead'))
  }
}
watch(() => [props.path, props.oldPath, props.staged], () => {
  picked.value = new Set()
  void load()
}, { immediate: true })
// Staging, a commit, a saved file: the diff is read again, and picks that are no longer changes go.
watch([changed, () => scan.value?.scannedAt], () => void load())

/* Picking lines */

const key = (line: DiffLine) => (line.kind === 'del' ? `o${line.old}` : `n${line.new}`)
/** The changed lines in order, for shift-click ranges. */
const changes = computed(() => (diff.value?.hunks ?? []).flatMap(hunk => hunk.lines.filter(line => line.kind !== 'context')))
const pickable = computed(() => !props.untracked && !props.conflicted && !diff.value?.binary && !(props.staged && diff.value?.hunks.length === 1 && diff.value.hunks[0]!.oldStart === 0))

function pick(line: DiffLine, event: MouseEvent): void {
  if (line.kind === 'context' || !pickable.value) return
  const at = changes.value.indexOf(line)
  const next = new Set(picked.value)
  if (event.shiftKey && lastPicked.value !== null) {
    const [from, to] = [Math.min(lastPicked.value, at), Math.max(lastPicked.value, at)]
    for (const item of changes.value.slice(from, to + 1)) next.add(key(item))
  } else if (next.has(key(line))) {
    next.delete(key(line))
  } else {
    next.add(key(line))
  }
  lastPicked.value = at
  picked.value = next
}

/* Picking by function: each changed line belongs to the innermost function of the scan around it (a removed line, to
   the one around the place it was removed from). */
interface FnRange { name: string, line: number, endLine: number }
const fileFunctions = computed<FnRange[]>(() => {
  const file = scan.value?.files.find(item => item.path === props.path)
  const out: FnRange[] = []
  const walk = (items: { name: string, line: number, endLine: number, anonymous: boolean, children: unknown[] }[]) => items.forEach((fn) => {
    if (!fn.anonymous) out.push({ name: fn.name, line: fn.line, endLine: fn.endLine })
    walk(fn.children as typeof items)
  })
  walk(file?.functions ?? [])
  return out
})
const byFunction = computed(() => {
  const groups = new Map<string, { name: string, keys: string[] }>()
  for (const hunk of diff.value?.hunks ?? []) {
    let cursor = hunk.newStart
    for (const line of hunk.lines) {
      const at = line.new ?? cursor
      if (line.new !== null) cursor = line.new + 1
      if (line.kind === 'context') continue
      const around = fileFunctions.value
        .filter(fn => fn.line <= at && at <= fn.endLine)
        .sort((a, b) => (a.endLine - a.line) - (b.endLine - b.line))[0]
      if (!around) continue
      const id = `${around.name}:${around.line}`
      const group = groups.get(id) ?? { name: around.name, keys: [] }
      group.keys.push(key(line))
      groups.set(id, group)
    }
  }
  return [...groups.values()]
})
function pickFunction(keys: string[]): void {
  const next = new Set(picked.value)
  const all = keys.every(item => next.has(item))
  for (const item of keys) {
    if (all) next.delete(item)
    else next.add(item)
  }
  picked.value = next
}

const pickedRefs = computed<LineRef[]>(() => changes.value
  .filter(line => picked.value.has(key(line)))
  .map(line => (line.kind === 'del' ? { old: line.old, new: null } : { old: null, new: line.new })))
const count = computed(() => pickedRefs.value.length)

async function act(action: 'stage' | 'unstage'): Promise<void> {
  const done = count.value
    ? await writeLines(props.path, pickedRefs.value, action)
    : await (action === 'stage' ? stage([props.path]) : unstage([props.path]))
  if (done) picked.value = new Set()
}
function discard(): void {
  emit('discard', count.value ? { lines: pickedRefs.value } : { file: true })
}
defineExpose({ clearPicks: () => { picked.value = new Set() } })

/* Showing it */

interface Piece { content: string, htmlStyle?: Record<string, string> }
const shown = computed(() => (diff.value?.hunks ?? []).map((hunk, index) => {
  const coloured = tokens.value[index]
  let oldAt = 0
  let newAt = 0
  return {
    hunk,
    lines: hunk.lines.map((line) => {
      const own = line.kind === 'add' ? coloured?.new?.[newAt] : coloured?.old?.[oldAt]
      if (line.kind !== 'add') oldAt++
      if (line.kind !== 'del') newAt++
      const pieces: Piece[] = own?.length ? own.map(token => ({ content: token.content, htmlStyle: token.htmlStyle as Record<string, string> | undefined })) : [{ content: line.text }]
      return { line, pieces }
    })
  }
}))
const BACKGROUND = { add: 'bg-success/10', del: 'bg-error/10', context: '' } as const
const SIGN = { add: '+', del: '−', context: ' ' } as const
</script>

<template>
  <div class="flex min-h-0 flex-col">
    <div class="flex flex-wrap items-center gap-2 border-b border-default px-3 py-1.5 text-xs">
      <span class="min-w-0 flex-1 truncate font-mono text-highlighted" :title="props.oldPath !== props.path ? `${props.oldPath} → ${props.path}` : props.path">
        <template v-if="props.oldPath !== props.path"><span class="text-muted">{{ props.oldPath }} → </span></template>{{ props.path }}
      </span>
      <span v-if="pickable && !count" class="text-muted">{{ t('LocalDiff.clickToPick') }}</span>
      <UButton v-if="!props.untracked" :label="t('LocalDiff.history')" icon="i-lucide-history" size="xs" color="neutral" variant="ghost" :to="{ path: '/history', query: { path: props.path } }" />
      <UButton v-if="!props.untracked" :label="t('LocalDiff.blame')" icon="i-lucide-user-round-search" size="xs" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path: props.path } }" />
      <UButton v-if="count" :label="t('LocalDiff.clearPicked', { count })" size="xs" color="neutral" variant="ghost" @click="picked = new Set()" />
      <template v-if="!props.staged">
        <UButton :label="count ? t('LocalDiff.stageLines', { count }) : t('LocalDiff.stageFile')" icon="i-lucide-plus" size="xs" :disabled="busy" @click="act('stage')" />
        <UButton :label="count ? t('LocalDiff.discardLines', { count }) : t('LocalDiff.discardFile')" icon="i-lucide-undo-2" size="xs" color="neutral" variant="outline" :disabled="busy" @click="discard" />
      </template>
      <UButton v-else :label="count ? t('LocalDiff.unstageLines', { count }) : t('LocalDiff.unstageFile')" icon="i-lucide-minus" size="xs" color="neutral" variant="outline" :disabled="busy" @click="act('unstage')" />
    </div>
    <div v-if="pickable && byFunction.length" class="flex flex-wrap items-center gap-1 border-b border-default px-3 py-1 text-xs">
      <span class="mr-1 text-muted">{{ t('LocalDiff.pickFunction') }}</span>
      <UButton
        v-for="group in byFunction"
        :key="group.name + group.keys[0]"
        :label="`${group.name} · ${group.keys.length}`"
        size="xs"
        :color="group.keys.every(item => picked.has(item)) ? 'primary' : 'neutral'"
        :variant="group.keys.every(item => picked.has(item)) ? 'soft' : 'ghost'"
        class="font-mono"
        :title="t('LocalDiff.pickFunctionLines', { count: group.keys.length, name: group.name })"
        @click="pickFunction(group.keys)"
      />
    </div>

    <p v-if="problem" class="p-3 text-sm text-error">{{ problem }}</p>
    <p v-else-if="props.conflicted" class="flex items-center gap-2 p-3 text-sm text-warning">
      {{ t('LocalDiff.conflicted') }}
      <UButton :label="t('LocalDiff.resolve')" size="xs" color="warning" :to="{ path: '/conflicts', query: { path: props.path } }" />
    </p>
    <p v-else-if="diff?.binary" class="p-3 text-sm text-muted">{{ t('LocalDiff.binary') }}</p>
    <p v-else-if="diff && !diff.hunks.length" class="p-3 text-sm text-muted">{{ t('LocalDiff.noLines') }}</p>

    <div v-else-if="diff" class="min-h-0 flex-1 overflow-auto font-mono text-xs leading-[1.6]">
      <table class="w-full border-collapse">
        <tbody v-for="({ hunk, lines }, index) in shown" :key="index">
          <tr class="bg-elevated/60 text-muted">
            <td colspan="5" class="px-3 py-0.5 text-label">
              <span class="text-dimmed">@@ −{{ hunk.oldStart }} +{{ hunk.newStart }} @@</span>
              <span v-if="hunk.section" class="ml-2 text-default">{{ hunk.section }}</span>
            </td>
          </tr>
          <tr
            v-for="({ line, pieces }, row) in lines"
            :key="row"
            :class="[BACKGROUND[line.kind], line.kind !== 'context' && pickable ? 'cursor-pointer hover:brightness-110' : '', picked.has(key(line)) && line.kind !== 'context' ? 'outline outline-1 -outline-offset-1 outline-primary/60' : '']"
            @click="pick(line, $event)"
          >
            <td class="w-5 text-center select-none">
              <UIcon v-if="line.kind !== 'context' && picked.has(key(line))" name="i-lucide-check" class="size-3 text-highlighted" />
            </td>
            <td class="w-12 px-2 text-right text-dimmed select-none">{{ line.old ?? '' }}</td>
            <td class="w-12 px-2 text-right text-dimmed select-none">{{ line.new ?? '' }}</td>
            <td class="w-4 text-center text-muted select-none">{{ SIGN[line.kind] }}</td>
            <td class="pr-4" :class="diffWrap ? 'break-all whitespace-pre-wrap' : 'whitespace-pre'" :style="{ tabSize: tabWidth }"><span v-for="(piece, at) in pieces" :key="at" class="code-token" :style="piece.htmlStyle">{{ piece.content }}</span></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else class="grid flex-1 place-items-center text-muted">
      <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
    </div>
  </div>
</template>
