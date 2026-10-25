<script setup lang="ts">
import { folderAt, folderTree, inside } from '~~/shared/folders'

/*
 * One part, folder by folder: its place in the map, a breadcrumb, the folders inside the one shown as tiles (files,
 * functions, a bar for their size), and the files directly in it with their functions. A big part opens on its top
 * folders instead of every file at once; arriving with a function picked, it opens on that function's folder.
 */
const props = defineProps<{ part: string }>()
const { structure: scan, index, visible, selected } = useCode()
const route = useRoute()
const router = useRouter()
const openAll = ref(true)

const part = computed(() => index.value?.parts.get(props.part) ?? null)
const layer = computed(() => scan.value?.layers.find(layer => layer.id === part.value?.layer))
const files = computed(() => (index.value?.filesByPart.get(props.part) ?? []).filter(visible))
const tree = computed(() => folderTree(files.value, part.value?.folder ?? ''))

/** The folder shown: the route's, else the picked function's (in this part), else the part's own. */
const path = computed(() => {
  if (typeof route.query.folder === 'string') return route.query.folder
  const file = selected.value?.file
  if (file && file.part === props.part) {
    const relative = inside(file.path, part.value?.folder ?? '')
    return relative.includes('/') ? relative.slice(0, relative.lastIndexOf('/') + 1) : ''
  }
  return ''
})
const folder = computed(() => folderAt(tree.value, path.value) ?? tree.value)
const crumbs = computed(() => {
  const out = [{ label: props.part, path: '' }]
  let at = ''
  for (const segment of folder.value.path.split('/').filter(Boolean)) {
    at += `${segment}/`
    out.push({ label: segment, path: at })
  }
  return out
})
/** Every file in the folder shown, folders inside it included: for who knows it. */
const folderPaths = computed(() => files.value.filter(file => inside(file.path, part.value?.folder ?? '').startsWith(folder.value.path)).map(file => file.path))
const largest = computed(() => Math.max(1, ...folder.value.folders.map(sub => sub.functionCount)))
const nested = computed(() => {
  let top = 0
  for (const file of files.value) top += file.functions.length
  return tree.value.functionCount - top
})

function go(to: string): void {
  void router.push({ query: { ...route.query, part: props.part, folder: to || undefined } })
}
</script>

<template>
  <section v-if="part" class="grid gap-4" :style="{ '--c': layerColor(part.layer) }">
    <div class="flex flex-wrap items-start justify-between gap-x-6 gap-y-2">
      <div class="min-w-0">
        <div class="font-mono text-xs tracking-wider text-(--c) uppercase">{{ layer?.label }} · {{ part.folder }}</div>
        <nav class="flex flex-wrap items-center gap-1 text-lg font-semibold" aria-label="Folder">
          <template v-for="(crumb, at) in crumbs" :key="crumb.path">
            <UIcon v-if="at" name="i-lucide-chevron-right" class="size-4 text-muted" />
            <button v-if="at < crumbs.length - 1" type="button" class="text-muted hover:text-highlighted" @click="go(crumb.path)">{{ crumb.label }}</button>
            <span v-else class="text-highlighted">{{ crumb.label }}</span>
          </template>
        </nav>
        <p v-if="!folder.path && (part.role || layer?.note)" class="text-sm text-muted">
          {{ part.role || layer?.note }}<template v-if="part.uses.length">. Also uses {{ part.uses.join(', ') }} from its own layer.</template>
        </p>
      </div>
      <div class="flex items-center gap-3 text-xs text-muted tabular-nums">
        <span>{{ folder.fileCount }} files · {{ folder.functionCount }} functions<template v-if="!folder.path"> · {{ nested }} nested</template></span>
        <USwitch v-if="folder.files.length" v-model="openAll" label="Open files" size="sm" />
      </div>
    </div>

    <PartKnowledge :paths="folderPaths" />

    <div v-if="folder.folders.length" class="grid grid-cols-[repeat(auto-fill,minmax(min(14rem,100%),1fr))] gap-2">
      <button
        v-for="sub in folder.folders"
        :key="sub.path"
        type="button"
        class="grid gap-1.5 rounded-md border border-default bg-elevated/40 px-3 py-2 text-left hover:border-(--c) hover:bg-elevated"
        :title="`${sub.path} · ${sub.lines} lines`"
        @click="go(sub.path)"
      >
        <span class="flex items-center gap-1.5 font-mono text-xs font-medium text-highlighted">
          <UIcon name="i-lucide-folder" class="size-3.5 shrink-0 text-(--c)" />
          <span class="truncate">{{ sub.name }}/</span>
        </span>
        <span class="text-label text-muted tabular-nums">{{ sub.fileCount }} files · {{ sub.functionCount }} functions</span>
        <span class="h-1 rounded-full bg-(--c)/70" :style="{ width: `${Math.max(4, (sub.functionCount / largest) * 100)}%` }" />
      </button>
    </div>

    <div v-if="folder.files.length" class="grid gap-2">
      <h3 v-if="folder.folders.length" class="font-mono text-xs text-muted">{{ folder.path || './' }} · {{ folder.files.length }} files</h3>
      <div class="grid grid-cols-[repeat(auto-fill,minmax(min(22rem,100%),1fr))] items-start gap-2.5">
        <FileCard v-for="file in folder.files" :key="`${file.path}-${openAll}`" :file="file" :label="file.path.split('/').pop()!" :open="openAll" />
      </div>
    </div>
  </section>
  <p v-else class="text-sm text-muted">No part called {{ props.part }} in this scan.</p>
</template>
