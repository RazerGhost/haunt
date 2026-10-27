<script setup lang="ts">
// A part's changes at a glance: how many of its functions changed (amber) and how many call a changed one (outlined).
const props = defineProps<{ counts?: { changed: number, affected: number } }>()
</script>

<template>
  <span v-if="props.counts && (props.counts.changed || props.counts.affected)" class="flex shrink-0 items-center gap-1 font-mono text-label tabular-nums" :title="t('ChangeBadge.title', { changed: props.counts.changed, affected: props.counts.affected })">
    <span v-if="props.counts.changed" class="rounded bg-warning/20 px-1 text-warning">{{ props.counts.changed }}Δ</span>
    <span v-if="props.counts.affected" class="rounded border border-warning/50 px-1 text-muted">{{ props.counts.affected }}</span>
  </span>
</template>
