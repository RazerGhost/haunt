<script setup lang="ts">
/*
 * The layer map: pick a part to see its files and functions, or search across all of them. In a wide window the map is
 * a sidebar and the part fills the rest; narrower, the map sits above the part.
 */
const { structure: scan, status, message, visible, search, selected } = useCode()
const { on: hotspots, parts: heat, days } = useHotspots()
const route = useRoute()
const router = useRouter()

const wide = ref(false)
const query = window.matchMedia('(min-width: 1280px)')
const follow = (event: MediaQueryListEvent) => { wide.value = event.matches }
onMounted(() => {
  wide.value = query.matches
  query.addEventListener('change', follow)
})
onBeforeUnmount(() => query.removeEventListener('change', follow))

const largest = computed(() => {
  const sizes = new Map<string, number>()
  for (const file of scan.value?.files ?? []) if (visible(file)) sizes.set(file.part, (sizes.get(file.part) ?? 0) + countFunctions(file.functions))
  return [...sizes].sort((a, b) => b[1] - a[1])[0]?.[0] ?? ''
})
const part = computed(() => {
  if (typeof route.query.part === 'string') return route.query.part
  // The part of the selected function, else the largest part.
  return selected.value?.file.part ?? largest.value
})
function choose(name: string) {
  search.value = ''
  // Another part opens on its own folder.
  void router.push({ query: { ...route.query, part: name, folder: undefined } })
}

const term = computed(() => search.value.trim().toLowerCase())
const found = computed(() => {
  if (!term.value || !scan.value) return []
  return scan.value.files.filter(file => visible(file) && matchesSearch(file.functions, term.value) > 0)
})
const hits = computed(() => found.value.reduce((sum, file) => sum + matchesSearch(file.functions, term.value), 0))
</script>

<template>
  <div v-if="!scan" class="grid justify-items-center gap-2 py-16 text-muted">
    <UIcon v-if="status !== 'error'" name="i-lucide-loader-circle" class="size-5 animate-spin" />
    <p>{{ message || 'Scanning' }}</p>
  </div>
  <div v-else :class="wide ? 'flex h-full' : 'grid gap-5 px-4 py-5'">
    <aside v-if="wide" class="flex w-72 shrink-0 flex-col border-r border-default">
      <div class="flex items-center justify-between border-b border-default px-3 py-2">
        <span class="text-xs font-semibold tracking-wider text-muted uppercase">Layers</span>
        <USwitch v-model="hotspots" label="Hotspots" size="xs" :title="`Tint parts by changes in the last ${days} days`" />
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-2">
        <LayerStack compact :selected-part="term ? null : part" :heat="heat" @select="choose" />
      </div>
    </aside>
    <template v-else>
      <div class="flex justify-end">
        <USwitch v-model="hotspots" label="Hotspots" size="xs" :title="`Tint parts by changes in the last ${days} days`" />
      </div>
      <LayerStack :selected-part="term ? null : part" :heat="heat" @select="choose" />
    </template>

    <div :class="wide ? 'grid min-w-0 flex-1 content-start gap-5 overflow-auto p-5' : 'contents'">
      <TrustLayersNote />
      <UAlert v-if="scan.architectureError" color="warning" variant="subtle" icon="i-lucide-triangle-alert" title="The layer map could not be read" :description="scan.architectureError" />
      <section v-if="term" class="grid gap-3">
        <h2 class="text-lg font-semibold text-highlighted">{{ hits }} functions named like “{{ search.trim() }}”, in {{ found.length }} files</h2>
        <div class="grid grid-cols-[repeat(auto-fill,minmax(min(22rem,100%),1fr))] items-start gap-2.5">
          <FileCard v-for="file in found.slice(0, 150)" :key="file.path" :file="file" :label="file.path" :term="term" open />
        </div>
        <p v-if="found.length > 150" class="text-sm text-muted">Showing the first 150 files. Narrow the search to see the rest.</p>
      </section>
      <PartFiles v-else :part="part" :class="wide ? '' : 'rounded-lg border border-default p-4'" />
    </div>
  </div>
</template>
