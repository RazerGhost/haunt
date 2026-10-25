<template>
  <div class="card">{{ title() }}</div>
</template>

<script setup lang="ts">
import { format } from '../lib/format'

const props = defineProps<{ n: number }>()

function title(): string {
  return format(props.n)
}
</script>
