<script setup lang="ts">
/*
 * The sidebar in Code: the views of the code, each with what it shows, and the project's parts by layer, a click
 * away from their files on the layer map. The git lists (branches, remotes, tags, stashes) stay in Git.
 */
const route = useRoute()
const { scan, status, message, visible, select } = useCode()
const pins = usePins()

const fn = computed(() => (typeof route.query.fn === 'string' ? route.query.fn : undefined))
const views = computed(() => [
  { label: t('CodeSidebar.layers'), icon: 'i-lucide-layers', what: t('CodeSidebar.layersWhat'), to: { path: '/layers', query: { fn: fn.value } }, path: '/layers' },
  { label: t('CodeSidebar.canvas'), icon: 'i-lucide-boxes', what: t('CodeSidebar.canvasWhat'), to: { path: '/canvas', query: { fn: fn.value } }, path: '/canvas' },
  { label: t('CodeSidebar.calls'), icon: 'i-lucide-git-fork', what: t('CodeSidebar.callsWhat'), to: { path: '/calls', query: { fn: fn.value } }, path: '/calls' },
  { label: t('CodeSidebar.imports'), icon: 'i-lucide-network', what: t('CodeSidebar.importsWhat'), to: { path: '/imports' }, path: '/imports' },
  { label: t('CodeSidebar.unused'), icon: 'i-lucide-ghost', what: t('CodeSidebar.unusedWhat'), to: { path: '/unused' }, path: '/unused' },
  { label: t('CodeSidebar.routes'), icon: 'i-lucide-route', what: t('CodeSidebar.routesWhat'), to: { path: '/routes' }, path: '/routes' }
])

/** The parts by layer, in the layers' order, with their functions counted (test files as Settings says). */
const layers = computed(() => {
  const current = scan.value
  if (!current) return []
  const counts = new Map<string, number>()
  const count = (items: { children: unknown[] }[]): number => items.reduce((sum, item) => sum + 1 + count(item.children as { children: unknown[] }[]), 0)
  for (const file of current.files) if (visible(file)) counts.set(file.part, (counts.get(file.part) ?? 0) + count(file.functions))
  return current.layers
    .map(layer => ({
      ...layer,
      parts: current.parts.filter(part => part.layer === layer.id && counts.has(part.name)).map(part => ({ name: part.name, functions: counts.get(part.name)! }))
    }))
    .filter(layer => layer.parts.length)
})
const partShown = computed(() => (route.path === '/layers' && typeof route.query.part === 'string' ? route.query.part : null))
const shut = usePersisted<Record<string, boolean>>('code-sidebar-shut', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
const toggle = (id: string) => { shut.value = { ...shut.value, [id]: !shut.value[id] } }
</script>

<template>
  <div class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
    <NuxtLink
      v-for="view in views"
      :key="view.path"
      :to="view.to"
      class="grid grid-cols-[1rem_minmax(0,1fr)] items-center gap-x-2 rounded-md px-2 py-1"
      :class="route.path === view.path ? 'bg-elevated text-highlighted' : 'text-default hover:bg-elevated/60'"
    >
      <UIcon :name="view.icon" class="size-4 text-muted" />
      <span class="truncate text-ui" :class="{ 'font-medium': route.path === view.path }">{{ view.label }}</span>
      <span class="col-start-2 truncate text-label text-muted">{{ view.what }}</span>
    </NuxtLink>
  </div>

  <section v-if="pins.pinned.value.length" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
    <p class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase"><UIcon name="i-lucide-pin" class="size-3" />{{ t('CodeSidebar.pinned') }}</p>
    <ul class="grid grid-cols-[minmax(0,1fr)]">
      <li v-for="pin in pins.pinned.value" :key="`${pin.path}#${pin.name}`" class="group flex items-center">
        <button
          type="button"
          class="flex min-w-0 flex-1 items-center gap-1.5 rounded py-1 pr-1 pl-5 text-left text-ui hover:bg-elevated disabled:cursor-default disabled:opacity-60"
          :disabled="!pin.found"
          :title="pin.found ? `${pin.path}:${pin.found.fn.line}` : t('CodeSidebar.gone', { name: pin.name, path: pin.path })"
          @click="pin.found && select(pin.found.fn.id)"
        >
          <span class="min-w-0 truncate font-mono" :class="{ 'line-through': !pin.found }">{{ pin.name }}</span>
          <span class="min-w-0 truncate text-label text-muted">{{ tailOf(pin.path) }}</span>
        </button>
        <button type="button" class="grid size-5 shrink-0 place-items-center rounded text-dimmed opacity-0 group-hover:opacity-100 hover:bg-elevated hover:text-highlighted" :aria-label="t('CodeSidebar.unpin', { name: pin.name })" @click="pins.unpin(pin)">
          <UIcon name="i-lucide-x" class="size-3" />
        </button>
      </li>
    </ul>
  </section>

  <p v-if="!scan" class="flex items-center gap-1.5 px-2 text-ui text-muted">
    <UIcon v-if="status !== 'error'" name="i-lucide-loader-circle" class="size-3.5 animate-spin" />{{ message || t('CodeSidebar.scanning') }}
  </p>
  <section v-for="layer in layers" :key="layer.id" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
    <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!shut[layer.id]" @click="toggle(layer.id)">
      <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !shut[layer.id] }" />
      <span class="size-2 shrink-0 rounded-full" :style="{ background: layerColor(layer.id) }" />
      <span class="truncate">{{ layer.label }}</span>
      <span class="ml-auto font-normal tabular-nums">{{ layer.parts.length }}</span>
    </button>
    <ul v-if="!shut[layer.id]" class="grid grid-cols-[minmax(0,1fr)]">
      <li v-for="part in layer.parts" :key="part.name">
        <NuxtLink
          :to="{ path: '/layers', query: { part: part.name } }"
          class="flex items-center gap-1.5 rounded py-1 pr-2 pl-5 text-ui hover:bg-elevated"
          :class="{ 'bg-elevated text-highlighted': partShown === part.name }"
          :title="`${part.name} · ${t('CodeSidebar.functions', { count: part.functions })}`"
        >
          <span class="min-w-0 flex-1 truncate">{{ part.name }}</span>
          <span class="text-label text-muted tabular-nums">{{ part.functions }}</span>
        </NuxtLink>
      </li>
    </ul>
  </section>
</template>
