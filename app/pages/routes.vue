<script setup lang="ts">
import type { Route } from '~/bindings'
import type { Located } from '~/composables/useCode'

/*
 * The HTTP routes in the code (src-tauri/src/code/routes.rs, sidecar/routes.ts): Hono and Express routes with their
 * mounted prefixes, and WordPress's REST routes, each with its method, path, middleware and handler. A handler is a
 * click from its call path (the Calls page's Paths view); an inline handler shows the functions it calls instead.
 * Found in the code as it is written: nothing runs, nothing leaves the machine. Try it sends a request to your dev
 * server on this machine (TryIt.vue).
 */
const { scan, index, select, open } = useCode()
const { repo } = useRepositories()
const router = useRouter()

const routes = shallowRef<Route[]>([])
const problem = ref('')
// A slow answer for the tab before lands nowhere.
const ask = latest(repo)
const { loading } = ask
async function load(): Promise<void> {
  if (!repo.value || !scan.value) return
  try {
    const found = await ask.run(at => haunt.routes(at))
    if (found === undefined) return
    routes.value = found
    problem.value = ''
  } catch (error) {
    problem.value = errorMessage(error, t('pages.routes.readFailed'))
  }
}
watch(repo, () => { routes.value = [] })
watch([repo, () => scan.value?.scannedAt], () => void load(), { immediate: true })

const filter = ref('')
const shown = computed(() => {
  const term = filter.value.trim().toLowerCase()
  return term ? routes.value.filter(route => `${route.method} ${route.path} ${route.handlerName} ${route.file}`.toLowerCase().includes(term)) : routes.value
})

/** The functions by the name a route calls them (`store`; a Go method `Order.store` also as `store`), built once per scan. */
const byName = computed(() => {
  const out = new Map<string, Located[]>()
  for (const item of index.value?.functions.values() ?? []) {
    const names = new Set([item.fn.name, item.fn.name.split('.').pop()!])
    for (const name of names) {
      const list = out.get(name)
      if (list) list.push(item)
      else out.set(name, [item])
    }
  }
  return out
})
/** Each route's handler: the scanner's id, or by its name in its file, else the only function of that name. */
const handlers = computed(() => {
  const out = new Map<Route, string | null>()
  for (const route of routes.value) {
    if (route.handler && index.value?.functions.has(route.handler)) {
      out.set(route, route.handler)
      continue
    }
    const named = route.handlerName === '(inline)' ? [] : byName.value.get(route.handlerName) ?? []
    out.set(route, (named.find(item => item.file.path === route.file) ?? (named.length === 1 ? named[0] : undefined))?.fn.id ?? null)
  }
  return out
})
const handlerOf = (route: Route): string | null => handlers.value.get(route) ?? null
/** Where a route leads: the functions its handler calls (or an inline handler's), a step down. */
function next(route: Route, handler: string | null): string[] {
  if (!handler) return route.calls.filter(id => index.value?.functions.has(id))
  return [...new Set((index.value?.callees.get(handler) ?? []).map(edge => edge.to))].slice(0, 4)
}
const name = (id: string) => index.value?.functions.get(id)?.fn.name ?? id
function paths(id: string): void {
  void router.push({ path: '/calls', query: { center: id, fn: id } })
}
const trying = ref(false)
const tried = shallowRef<Route | null>(null)
function tryIt(route: Route): void {
  tried.value = route
  trying.value = true
}
const METHOD: Record<string, 'success' | 'info' | 'warning' | 'error' | 'neutral'> = { GET: 'success', POST: 'info', PUT: 'warning', PATCH: 'warning', DELETE: 'error' }
const colorOf = (method: string) => METHOD[method.split(',')[0]!.trim()] ?? 'neutral'
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-3 border-b border-default px-4 py-2">
      <h1 class="text-sm font-semibold text-highlighted">{{ t('pages.routes.title') }}</h1>
      <span class="text-xs text-muted tabular-nums">{{ routes.length }}</span>
      <UInput v-model="filter" icon="i-lucide-filter" :placeholder="t('pages.routes.filter')" size="xs" class="w-72" />
      <span class="flex-1" />
      <UButton icon="i-lucide-refresh-cw" size="xs" color="neutral" variant="ghost" :aria-label="t('pages.routes.reload')" :loading="loading" @click="load" />
    </div>
    <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
      <p v-if="problem" class="text-sm text-error">{{ problem }}</p>
      <p v-else-if="!scan" class="text-sm text-muted">{{ t('pages.routes.scanning') }}</p>
      <div v-else-if="!routes.length && !loading" class="grid max-w-xl gap-1 text-sm text-muted">
        <p class="font-medium text-highlighted">{{ t('pages.routes.none') }}</p>
        <p>{{ t('pages.routes.noneHelp.before') }}<code>app.get('/path', handler)</code>{{ t('pages.routes.noneHelp.with') }} <code>route()</code>, <code>use()</code> {{ t('pages.routes.noneHelp.and') }} <code>basePath()</code> {{ t('pages.routes.noneHelp.prefixes') }} <code>register_rest_route</code>{{ t('pages.routes.noneHelp.after') }}</p>
      </div>
      <ul v-else class="grid gap-1">
        <li v-for="route in shown" :key="`${route.file}:${route.line}:${route.method}:${route.path}`" class="grid gap-1 rounded border border-default px-3 py-2">
          <div class="flex min-w-0 flex-wrap items-center gap-2">
            <UBadge :label="route.method" size="sm" :color="colorOf(route.method)" variant="subtle" class="font-mono" />
            <span class="min-w-0 truncate font-mono text-sm text-highlighted" :title="route.path">{{ route.path }}</span>
            <UBadge v-for="item in route.middleware" :key="item" :label="item" size="sm" color="neutral" variant="outline" class="font-mono" />
            <span class="flex-1" />
            <button type="button" class="font-mono text-label text-muted hover:underline" :title="t('pages.routes.open', { file: route.file })" @click="open(route.file, route.line)">{{ route.file }}:{{ route.line }}</button>
          </div>
          <div class="flex min-w-0 flex-wrap items-center gap-1.5 text-xs text-muted">
            <template v-if="handlerOf(route)">
              <button type="button" class="font-mono text-default hover:underline" @click="select(handlerOf(route))">{{ name(handlerOf(route)!) }}</button>
            </template>
            <span v-else class="font-mono">{{ route.handlerName }}</span>
            <template v-for="id in next(route, handlerOf(route))" :key="id">
              <UIcon name="i-lucide-arrow-right" class="size-3" />
              <button type="button" class="font-mono hover:text-default hover:underline" @click="select(id)">{{ name(id) }}</button>
            </template>
            <span class="flex-1" />
            <UButton v-if="handlerOf(route) || route.calls.length" :label="t('pages.routes.callPath')" icon="i-lucide-git-fork" size="xs" color="neutral" variant="ghost" @click="paths(handlerOf(route) ?? route.calls[0]!)" />
            <UButton :label="t('pages.routes.tryIt')" icon="i-lucide-send" size="xs" color="neutral" variant="ghost" :title="t('pages.routes.tryItHelp')" @click="tryIt(route)" />
          </div>
        </li>
      </ul>
    </div>
    <TryIt v-model:open="trying" :route="tried" :handler="tried ? handlerOf(tried) : null" />
  </div>
</template>
