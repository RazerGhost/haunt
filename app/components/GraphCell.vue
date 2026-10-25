<script setup lang="ts">
import type { GraphRow } from '~~/shared/graph'

// One row's piece of the commit graph: the lines passing through, into and out of the commit, and its dot.
const props = defineProps<{ row: GraphRow, width: number, merge: boolean, head: boolean }>()

const ROW = 26
const LANE = 14
const PALETTE = ['#3b82f6', '#f59e0b', '#10b981', '#ef4444', '#8b5cf6', '#06b6d4', '#ec4899', '#84cc16']
const x = (lane: number) => lane * LANE + LANE / 2
const colour = (index: number) => PALETTE[index % PALETTE.length]

const paths = computed(() => props.row.lines.map((line) => {
  const [y1, y2] = line.part === 'top' ? [0, ROW / 2] : line.part === 'bottom' ? [ROW / 2, ROW] : [0, ROW]
  const [x1, x2] = [x(line.from), x(line.to)]
  const middle = (y1 + y2) / 2
  // A lane changing columns bends smoothly; a straight lane is a straight line.
  const d = x1 === x2 ? `M${x1} ${y1}V${y2}` : `M${x1} ${y1}C${x1} ${middle} ${x2} ${middle} ${x2} ${y2}`
  return { d, stroke: colour(line.color) }
}))
</script>

<template>
  <svg :width="props.width * LANE" :height="ROW" class="shrink-0 overflow-visible" aria-hidden="true">
    <path v-for="(path, at) in paths" :key="at" :d="path.d" :stroke="path.stroke" stroke-width="2" fill="none" />
    <circle :cx="x(props.row.lane)" :cy="ROW / 2" :r="props.head ? 5 : 4" :fill="props.merge ? 'var(--ui-bg)' : colour(props.row.color)" :stroke="colour(props.row.color)" stroke-width="2" />
  </svg>
</template>
