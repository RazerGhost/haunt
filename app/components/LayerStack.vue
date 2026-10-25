<script setup lang="ts">
/*
 * The bands of the layer map, top to bottom, with a tile per part sized by how many functions it has. Compact: a list per
 * band, for the sidebar of a wide window. A part with changes (useChanges) shows how many functions changed and how many
 * call them; with `heat`, a tile is tinted by how much its files changed lately (hotspots).
 */
const props = defineProps<{ selectedPart: string | null, compact?: boolean, heat?: Map<string, { score: number, commits: number }> | null, counts?: Map<string, { changed: number, affected: number }> }>()
const emit = defineEmits<{ select: [part: string] }>()
const { scan, index, visible, search } = useCode()
const { byPart } = useChanges()
// The badges: the local changes, unless a commit's lens passes its own counts.
const counts = computed(() => props.counts ?? byPart.value)

interface Tile { name: string, files: number, functions: number, topLevel: number, hits: number, role: string }

const bands = computed(() => {
  if (!scan.value || !index.value) return []
  const term = search.value.trim().toLowerCase()
  return scan.value.layers.map((layer) => {
    const tiles: Tile[] = scan.value!.parts.filter(part => part.layer === layer.id).map((part) => {
      const files = (index.value!.filesByPart.get(part.name) ?? []).filter(visible)
      const functions = files.reduce((sum, file) => sum + countFunctions(file.functions), 0)
      const topLevel = files.reduce((sum, file) => sum + file.functions.length, 0)
      const hits = term ? files.reduce((sum, file) => sum + matchesSearch(file.functions, term), 0) : 0
      return { name: part.name, files: files.length, functions, topLevel, hits, role: part.role }
    }).filter(tile => tile.files > 0).sort((a, b) => b.functions - a.functions)
    return { layer, tiles, total: tiles.reduce((sum, tile) => sum + tile.functions, 0) }
  }).filter(band => band.tiles.length)
})
const searching = computed(() => !!search.value.trim())
const short = (name: string) => name.replace(/^@[^/]+\//, '')

/** A tile's background: hotter parts warmer, from nothing to a clear amber. */
function tint(name: string): Record<string, string> {
  const hot = props.heat?.get(name)
  return hot ? { background: `color-mix(in srgb, var(--rule-known) ${Math.round(6 + hot.score * 38)}%, var(--ui-bg))` } : {}
}
function heatTitle(name: string): string {
  const hot = props.heat?.get(name)
  return hot ? ` · ${hot.commits} file changes in the last 180 days` : ''
}
</script>

<template>
  <section v-if="!props.compact" class="grid gap-1.5" aria-label="Layers">
    <div v-for="band in bands" :key="band.layer.id" class="grid gap-2.5 sm:grid-cols-[9rem_1fr]" :style="{ '--c': layerColor(band.layer.id) }">
      <div class="flex flex-col justify-center rounded-r-md border-l-4 border-(--c) bg-elevated/50 px-2.5 py-1.5">
        <span class="text-xs font-semibold tracking-wider text-(--c) uppercase">{{ band.layer.label }}</span>
        <span class="text-xs text-muted tabular-nums">{{ band.tiles.length }} parts · {{ band.total }} fns</span>
      </div>
      <div class="flex min-w-0 flex-wrap gap-1.5">
        <button
          v-for="tile in band.tiles"
          :key="tile.name"
          type="button"
          class="grid min-w-24 gap-1 rounded-md border border-t-[3px] border-default border-t-(--c) bg-default px-2 py-1.5 text-left transition hover:border-(--c) focus-visible:outline-2 focus-visible:outline-(--c)"
          :class="{ 'border-(--c) bg-elevated!': props.selectedPart === tile.name, 'opacity-35': searching && !tile.hits }"
          :style="{ flex: `${Math.max(1, Math.round(Math.sqrt(tile.functions)))} 1 6rem`, ...tint(tile.name) }"
          :title="(tile.role || band.layer.note) + heatTitle(tile.name)"
          :aria-pressed="props.selectedPart === tile.name"
          @click="emit('select', tile.name)"
        >
          <span class="flex items-start justify-between gap-1">
            <span class="font-mono text-xs font-medium break-all">{{ short(tile.name) }}</span>
            <ChangeBadge :counts="counts.get(tile.name)" />
          </span>
          <span class="text-label text-muted tabular-nums">
            {{ tile.files }} files · {{ tile.functions }} fns<template v-if="searching"> · <b class="text-highlighted">{{ tile.hits }} found</b></template>
          </span>
          <span class="flex h-1 overflow-hidden rounded-full bg-elevated" :title="`${tile.topLevel} at file level, ${tile.functions - tile.topLevel} nested`">
            <i class="block h-full bg-(--c)" :style="{ width: `${tile.functions ? tile.topLevel / tile.functions * 100 : 0}%` }" />
            <i class="block h-full bg-(--c) opacity-40" :style="{ width: `${tile.functions ? (tile.functions - tile.topLevel) / tile.functions * 100 : 0}%` }" />
          </span>
        </button>
      </div>
    </div>
  </section>

  <nav v-else class="grid gap-3" aria-label="Layers">
    <section v-for="band in bands" :key="band.layer.id" class="grid gap-0.5" :style="{ '--c': layerColor(band.layer.id) }">
      <h3 class="flex items-baseline justify-between border-l-2 border-(--c) px-2 py-0.5">
        <span class="text-label font-semibold tracking-wider text-(--c) uppercase">{{ band.layer.label }}</span>
        <span class="text-label text-muted tabular-nums">{{ band.total }} fns</span>
      </h3>
      <button
        v-for="tile in band.tiles"
        :key="tile.name"
        type="button"
        class="grid gap-0.5 rounded px-2 py-1 text-left transition hover:bg-elevated focus-visible:outline-2 focus-visible:outline-(--c)"
        :class="{ 'bg-elevated ring-1 ring-(--c)': props.selectedPart === tile.name, 'opacity-35': searching && !tile.hits }"
        :style="tint(tile.name)"
        :title="(tile.role || band.layer.note) + heatTitle(tile.name)"
        :aria-pressed="props.selectedPart === tile.name"
        @click="emit('select', tile.name)"
      >
        <span class="flex min-w-0 items-center justify-between gap-2">
          <span class="truncate font-mono text-xs" :class="props.selectedPart === tile.name ? 'font-semibold text-highlighted' : ''">{{ short(tile.name) }}</span>
          <span class="flex shrink-0 items-center gap-1.5">
            <ChangeBadge :counts="counts.get(tile.name)" />
            <span class="text-label text-muted tabular-nums">{{ searching ? `${tile.hits} found` : tile.functions }}</span>
          </span>
        </span>
        <span class="flex h-0.5 overflow-hidden rounded-full bg-elevated">
          <i class="block h-full bg-(--c)" :style="{ width: `${band.total ? Math.max(4, tile.functions / bands.reduce((m, b) => Math.max(m, ...b.tiles.map(t => t.functions)), 1) * 100) : 0}%` }" />
        </span>
      </button>
    </section>
  </nav>
</template>
