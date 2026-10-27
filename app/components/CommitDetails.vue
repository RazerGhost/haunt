<script setup lang="ts">
import type { ElementDefinition } from 'cytoscape'
import type { ChangedFile, ChangedPath, CommitDetail, RangeChanges } from '~~/shared/types'
import type { Signature } from '~/bindings'

/*
 * The change picked in the commit list: one commit (against its first parent) or two compared (older to newer), with
 * six lenses. Files: the diff. Functions: changes by function, both versions read from git. What could break, Map and
 * Calls: through the call graph of the code as it is now, so a function that no longer exists has no callers here.
 * Architecture: the imports the change adds and removes, judged by the project's rules (ArchitectureChanges).
 */
/** `alone`: shown outside the commit list (a pull request), so its hints about picking commits there are left out. */
const props = defineProps<{ sha: string, compare: string | null, alone?: boolean }>()
const emit = defineEmits<{ select: [sha: string] }>()
const { scan, index, select } = useCode()
const { repo } = useRepositories()
const colorMode = useColorMode()
const router = useRouter()

const detail = shallowRef<CommitDetail | null>(null)
/* Its signature, checked after the rest is shown (checking runs gpg or ssh-keygen, which can take a moment). */
const signature = shallowRef<Signature | null>(null)
const signatureAsk = latest(repo)
watch(() => detail.value?.sha, async (sha) => {
  signature.value = null
  if (!sha) return
  try {
    const found = await signatureAsk.run(at => haunt.commitSignature(at, sha))
    if (found !== undefined && detail.value?.sha === sha) signature.value = found
  } catch { /* shown without it */ }
})
const SIGNATURE_LOOK: Record<Signature['status'], { color: 'success' | 'warning' | 'error' | 'neutral', icon: string }> = {
  good: { color: 'success', icon: 'i-lucide-shield-check' },
  unknown: { color: 'neutral', icon: 'i-lucide-shield-question' },
  expired: { color: 'warning', icon: 'i-lucide-shield-alert' },
  revoked: { color: 'error', icon: 'i-lucide-shield-x' },
  bad: { color: 'error', icon: 'i-lucide-shield-x' },
  unchecked: { color: 'neutral', icon: 'i-lucide-shield-question' },
  none: { color: 'neutral', icon: 'i-lucide-shield' }
}
const other = shallowRef<CommitDetail | null>(null)
const rangeFiles = shallowRef<ChangedPath[] | null>(null)
const problem = ref('')
const ask = latest(repo)

async function load(): Promise<void> {
  problem.value = ''
  try {
    const found = await ask.run(async (at) => {
      const [main, second] = await Promise.all([haunt.commit(at, props.sha), props.compare ? haunt.commit(at, props.compare) : Promise.resolve(null)])
      if (!second) return { main, second, files: null }
      const [older, newer] = second.time <= main.time ? [second, main] : [main, second]
      return { main, second, files: await haunt.files(at, older.sha, newer.sha) }
    })
    if (found === undefined) return
    detail.value = found.main
    other.value = found.second
    rangeFiles.value = found.files
  } catch (error) {
    problem.value = errorMessage(error, t('CommitDetails.readError'))
  }
}
watch(() => [props.sha, props.compare], () => void load(), { immediate: true })

/** What is compared: a commit with its first parent, or the older of two commits with the newer. */
const span = computed(() => {
  if (!detail.value) return null
  if (other.value) {
    const [older, newer] = other.value.time <= detail.value.time ? [other.value, detail.value] : [detail.value, other.value]
    return { from: older.sha as string | null, to: newer.sha, files: rangeFiles.value ?? [] }
  }
  return { from: detail.value.parents[0] ?? null, to: detail.value.sha, files: detail.value.files }
})

// Each lens's label is `CommitDetails.lens.<value>`.
const LENSES = [
  { value: 'files', icon: 'i-lucide-file-diff' },
  { value: 'functions', icon: 'i-lucide-braces' },
  { value: 'impact', icon: 'i-lucide-triangle-alert' },
  { value: 'map', icon: 'i-lucide-layers' },
  { value: 'calls', icon: 'i-lucide-git-fork' },
  { value: 'architecture', icon: 'i-lucide-network' }
] as const
type Lens = (typeof LENSES)[number]['value']
// The lens is kept per repository (each tab comes back to its own).
const lenses = usePersisted<Record<string, Lens>>('lenses', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
const lens = computed<Lens>({
  get: () => {
    const kept = repo.value ? lenses.value[repo.value] : undefined
    return kept && LENSES.some(item => item.value === kept) ? kept : 'files'
  },
  set: (value) => { if (repo.value) lenses.value = { ...lenses.value, [repo.value]: value } }
})

/* Files */
const file = ref<ChangedPath | null>(null)
watch(span, (now) => {
  file.value = now?.files.find(item => item.path === file.value?.path) ?? now?.files[0] ?? null
})
const STATUS_ICON: Record<ChangedPath['status'], { icon: string, class: string }> = {
  added: { icon: 'i-lucide-plus', class: 'text-success' },
  modified: { icon: 'i-lucide-pencil', class: 'text-warning' },
  deleted: { icon: 'i-lucide-minus', class: 'text-error' },
  renamed: { icon: 'i-lucide-arrow-right', class: 'text-info' },
  copied: { icon: 'i-lucide-copy', class: 'text-info' },
  type: { icon: 'i-lucide-replace', class: 'text-muted' }
}

/* Functions, read from git once a lens needs them */
const changes = shallowRef<RangeChanges | null>(null)
const changesProblem = ref('')
const changesLoading = ref(false)
const askChanges = latest(repo)
// Asked once the first structure is in: the scanner answers after it, and a call left waiting holds one of the few
// connections the window has, so the files and diffs would wait behind it.
watch([span, lens, () => !!scan.value], async ([now, current, scanned]) => {
  if (!now || current === 'files' || !scanned) return
  if (changes.value && changes.value.from === now.from && changes.value.to === now.to) return
  changesLoading.value = true
  changesProblem.value = ''
  try {
    const found = await askChanges.run(at => haunt.range(at, now.from, now.to))
    if (found === undefined) return
    changes.value = found
    changesLoading.value = false
  } catch (error) {
    changesProblem.value = errorMessage(error, t('CommitDetails.changesError'))
    changesLoading.value = false
  }
}, { immediate: true })
const codeFiles = computed<ChangedFile[]>(() => (changes.value && changes.value.to === span.value?.to ? changes.value.files.filter(item => item.code) : []))
const depth = ref(2)
const ids = computed(() => changedIds(codeFiles.value))
const affected = computed(() => namedOnly(index.value, blastRadius(index.value, ids.value, depth.value)))
const counts = computed(() => countsByPart(index.value, codeFiles.value, affected.value))
/* The Map lens as the layer map or the canvas; the canvas zooms to the files the change touched. */
const mapAs = usePersisted<'layers' | 'canvas'>('map-as', 'layers', value => value === 'layers' || value === 'canvas')
const affectedIds = computed(() => new Set(affected.value.keys()))
const touchedFiles = computed(() => codeFiles.value.filter(item => item.functions.some(fn => fn.id)).map(item => item.path))
const missing = computed(() => codeFiles.value.reduce((sum, item) => sum + item.functions.filter(fn => !fn.id && fn.status !== 'removed').length, 0))

function showFile(changed: ChangedFile): void {
  file.value = span.value?.files.find(item => item.path === changed.path) ?? null
  lens.value = 'files'
}

/* Calls: the changed functions in the middle, their callers to the left and what they call to the right. */
const PER_COLUMN = 30
const graph = computed(() => {
  void colorMode.value
  const at = index.value
  if (!at || lens.value !== 'calls') return [] as ElementDefinition[]
  const middle = [...ids.value].filter(id => at.functions.has(id)).slice(0, PER_COLUMN)
  const shown = new Set(middle)
  const side = (next: (id: string) => string[]) => {
    const found = new Map<string, number>()
    for (const id of middle) for (const other of next(id)) if (!shown.has(other) && at.functions.has(other)) found.set(other, (found.get(other) ?? 0) + 1)
    const ranked = [...found].sort((a, b) => b[1] - a[1]).slice(0, PER_COLUMN).map(([id]) => id)
    ranked.forEach(id => shown.add(id))
    return ranked
  }
  const callers = side(id => (at.callers.get(id) ?? []).map(edge => edge.from))
  const callees = side(id => (at.callees.get(id) ?? []).map(edge => edge.to))
  const elements: ElementDefinition[] = []
  for (const [x, column, classes] of [[-1, callers, 'affected'], [0, middle, 'changed'], [1, callees, '']] as const) {
    column.forEach((id, row) => {
      const located = at.functions.get(id)!
      const label = `${located.fn.name}\n${tailOf(located.file.path)}`
      const layer = at.parts.get(located.file.part)?.layer ?? 'unmapped'
      elements.push({ group: 'nodes', data: { id, label, color: resolvedColor(layerVar(layer)), ...nodeSize(label) }, position: { x: x * 300, y: (row - (column.length - 1) / 2) * 58 }, classes })
    })
  }
  const line = resolvedColor('--ui-text-dimmed')
  for (const id of shown) {
    for (const edge of at.callees.get(id) ?? []) {
      if (shown.has(edge.to)) elements.push({ group: 'edges', data: { id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to, width: Math.min(6, 1 + Math.log2(edge.count)), color: line }, classes: edge.guessed ? 'dashed' : '' })
    }
  }
  return elements
})
const layout = { name: 'preset' as const, fit: true, padding: 40 }

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
  } catch { /* no clipboard: nothing to do */ }
}
</script>

<template>
  <div class="flex min-h-0 flex-col">
    <UAlert v-if="problem" class="m-3" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    <template v-else-if="detail && span">
      <header class="grid max-h-40 shrink-0 gap-1 overflow-auto border-b border-default px-4 py-2">
        <template v-if="other">
          <p class="text-sm font-medium text-highlighted">
            {{ t('CommitDetails.comparing') }} <span class="font-mono">{{ span.from?.slice(0, 7) }}</span> → <span class="font-mono">{{ span.to.slice(0, 7) }}</span>
          </p>
          <p class="text-xs text-muted">{{ t('CommitDetails.filesBetween', { count: span.files.length }) }}<template v-if="!props.alone"> {{ t('CommitDetails.compareOther') }}</template></p>
        </template>
        <template v-else>
          <div class="flex items-start gap-2">
            <p class="min-w-0 flex-1 text-sm font-medium text-highlighted">{{ detail.subject }}</p>
            <UButton :label="detail.sha.slice(0, 7)" icon="i-lucide-copy" size="xs" color="neutral" variant="ghost" class="font-mono" :title="t('CommitDetails.copyId')" @click="copy(detail.sha)" />
          </div>
          <p v-if="detail.body" class="max-h-16 overflow-auto text-xs whitespace-pre-wrap text-muted">{{ detail.body }}</p>
          <p class="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted">
            <span :title="detail.email">{{ detail.author }}</span>
            <span>{{ new Date(detail.time * 1000).toLocaleString(locale()) }}</span>
            <span v-if="detail.committer !== detail.author">{{ t('CommitDetails.committedBy', { name: detail.committer }) }}</span>
            <UBadge
              v-if="signature && signature.status !== 'none'"
              :label="t(`CommitDetails.signature.${signature.status}`)"
              :icon="SIGNATURE_LOOK[signature.status].icon"
              :color="SIGNATURE_LOOK[signature.status].color"
              variant="subtle"
              size="sm"
              :title="[t(`CommitDetails.signatureHelp.${signature.status}`), signature.signer && t('CommitDetails.signedBy', { signer: signature.signer }), signature.key && t('CommitDetails.signedWith', { format: signature.format ?? '', key: signature.key })].filter(Boolean).join('\n')"
            />
            <span v-if="detail.parents.length" class="flex items-center gap-1">
              {{ t('CommitDetails.parents', { count: detail.parents.length }) }}
              <button v-for="parent in detail.parents" :key="parent" type="button" class="font-mono text-default hover:underline" @click="emit('select', parent)">{{ parent.slice(0, 7) }}</button>
            </span>
            <span v-else>{{ t('CommitDetails.firstCommit') }}</span>
            <span v-if="detail.parents.length > 1" class="text-dimmed">{{ t('CommitDetails.firstParent') }}</span>
          </p>
        </template>
        <!-- Once the change is read by function (any lens but Files). -->
        <RiskLine v-if="codeFiles.length" :files="codeFiles" />
      </header>

      <nav class="flex items-center gap-0.5 border-b border-default px-2" :aria-label="t('CommitDetails.lenses')">
        <button
          v-for="item in LENSES"
          :key="item.value"
          type="button"
          class="flex items-center gap-1.5 border-b-2 px-2.5 py-1.5 text-xs"
          :class="lens === item.value ? 'border-(--ui-primary) font-medium text-highlighted' : 'border-transparent text-muted hover:text-default'"
          :aria-current="lens === item.value ? 'page' : undefined"
          @click="lens = item.value"
        >
          <UIcon :name="item.icon" class="size-3.5" />{{ t(`CommitDetails.lens.${item.value}`) }}
          <span v-if="item.value === 'files'" class="text-dimmed tabular-nums">{{ span.files.length }}</span>
        </button>
        <span v-if="lens !== 'files' && changesLoading" class="ml-auto"><UIcon name="i-lucide-loader-circle" class="size-3.5 animate-spin text-muted" /></span>
      </nav>

      <!-- Files -->
      <div v-if="lens === 'files'" class="grid min-h-0 flex-1 grid-cols-[18rem_minmax(0,1fr)]">
        <ul class="min-h-0 overflow-auto border-r border-default py-1">
          <li v-for="item in span.files" :key="item.path">
            <button
              type="button"
              class="flex w-full items-center gap-1.5 px-2 py-0.5 text-left text-xs"
              :class="file?.path === item.path ? 'bg-elevated text-highlighted' : 'hover:bg-elevated/50'"
              :title="item.status === 'renamed' ? `${item.oldPath} → ${item.path}` : item.path"
              @click="file = item"
            >
              <UIcon :name="STATUS_ICON[item.status].icon" class="size-3 shrink-0" :class="STATUS_ICON[item.status].class" />
              <PathLabel :path="item.path" class="flex-1 font-mono" />
              <span class="font-mono text-label tabular-nums"><span class="text-success">+{{ item.added }}</span> <span class="text-error">−{{ item.removed }}</span></span>
            </button>
          </li>
        </ul>
        <DiffView v-if="file" :key="`${span.from}:${span.to}:${file.path}`" :from="span.from" :to="span.to" :path="file.path" :old-path="file.oldPath" class="min-h-0" />
        <p v-else class="p-4 text-sm text-muted">{{ t('CommitDetails.noFiles') }}</p>
      </div>

      <!-- Architecture: the imports the change adds and removes, judged by the rules (it needs no changes by function) -->
      <div v-else-if="lens === 'architecture'" class="min-h-0 flex-1 overflow-auto p-4">
        <ArchitectureChanges :from="span.from" :to="span.to" />
      </div>

      <template v-else>
        <UAlert v-if="changesProblem" class="m-3" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="changesProblem" />
        <div v-else-if="!changes || changes.to !== span.to" class="grid flex-1 place-items-center gap-2 text-muted">
          <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
          <p v-if="!scan" class="text-xs">{{ t('CommitDetails.waitingScan') }}</p>
        </div>

        <!-- Functions -->
        <div v-else-if="lens === 'functions'" class="min-h-0 flex-1 overflow-auto p-4">
          <p v-if="!codeFiles.length" class="text-sm text-muted">{{ t('CommitDetails.noCode') }}</p>
          <ChangedFunctions v-else :files="codeFiles" @file="showFile" />
        </div>

        <!-- What could break -->
        <div v-else-if="lens === 'impact'" class="min-h-0 flex-1 overflow-auto p-4">
          <div class="mb-3 flex flex-wrap items-center gap-2 text-xs text-muted">
            <span class="flex-1">{{ t('CommitDetails.impactHelp') }}<template v-if="missing"> {{ t('CommitDetails.missing', { count: missing }) }}</template></span>
            {{ t('CommitDetails.stepsBefore') }}
            <UFieldGroup size="xs">
              <UButton v-for="step in [1, 2, 3]" :key="step" :label="`${step}`" :color="depth === step ? 'primary' : 'neutral'" :variant="depth === step ? 'solid' : 'outline'" @click="depth = step" />
            </UFieldGroup>
            {{ t('CommitDetails.stepsAfter') }}
          </div>
          <AffectedList :affected="affected" />
          <TestsReached :changed="ids" class="mt-6" />
          <SuggestedReviewers :base="span.from" :files="codeFiles" :exclude="[detail?.author, other?.author].filter((name): name is string => !!name)" class="mt-6" />
        </div>

        <!-- Map: the layer map, or the canvas -->
        <div v-else-if="lens === 'map'" class="flex min-h-0 flex-1 flex-col">
          <div class="flex items-center gap-3 px-4 pt-3 pb-2">
            <p class="min-w-0 flex-1 text-xs text-muted">
              {{ mapAs === 'layers' ? t('CommitDetails.mapLayersHelp') : t('CommitDetails.mapCanvasHelp') }}
            </p>
            <UFieldGroup size="xs">
              <UButton :label="t('CommitDetails.layers')" icon="i-lucide-layers" :color="mapAs === 'layers' ? 'primary' : 'neutral'" :variant="mapAs === 'layers' ? 'solid' : 'outline'" @click="mapAs = 'layers'" />
              <UButton :label="t('CommitDetails.canvas')" icon="i-lucide-boxes" :color="mapAs === 'canvas' ? 'primary' : 'neutral'" :variant="mapAs === 'canvas' ? 'solid' : 'outline'" @click="mapAs = 'canvas'" />
            </UFieldGroup>
          </div>
          <div v-if="mapAs === 'layers'" class="min-h-0 flex-1 overflow-auto px-4 pb-4">
            <LayerStack :selected-part="null" :counts="counts" @select="part => router.push({ path: '/layers', query: { part } })" />
          </div>
          <div v-else class="min-h-0 flex-1 border-t border-default">
            <CodeCanvas :changed="ids" :affected="affectedIds" :focus="touchedFiles" />
          </div>
        </div>

        <!-- Calls -->
        <div v-else class="relative min-h-0 flex-1">
          <p v-if="!graph.length" class="absolute inset-0 grid place-items-center text-sm text-muted">{{ index?.callers.size ? t('CommitDetails.noneInCode') : t('CommitDetails.workingOut') }}</p>
          <GraphView v-else :elements="graph" :stylesheet="graphStyle" :layout="layout" @node="id => select(id)" @open="id => router.push({ path: '/calls', query: { center: id, fn: id } })" />
          <p class="pointer-events-none absolute right-3 bottom-2 text-label text-muted">{{ t('CommitDetails.callsHint') }}</p>
        </div>
      </template>
    </template>
    <div v-else class="grid flex-1 place-items-center text-muted">
      <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
    </div>
  </div>
</template>
