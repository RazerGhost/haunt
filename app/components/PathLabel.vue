<script setup lang="ts">
/*
 * A file's path in a narrow list: its folder dimmed, then its name. When the room runs out the folder is cut first, so
 * the name, the part you look for, stays whole as long as it fits on its own.
 */
const props = defineProps<{ path: string }>()
const folder = computed(() => (props.path.includes('/') ? props.path.slice(0, props.path.lastIndexOf('/') + 1) : ''))
const name = computed(() => props.path.slice(props.path.lastIndexOf('/') + 1))
</script>

<template>
  <span class="flex min-w-0"><span v-if="folder" class="min-w-0 truncate text-muted">{{ folder }}</span><span class="min-w-0 shrink-0 basis-auto truncate" :class="folder ? 'max-w-full' : ''">{{ name }}</span></span>
</template>
