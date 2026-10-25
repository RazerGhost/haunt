<script setup lang="ts">
import type { HttpFile, HttpRequest, HttpResponse, Route } from '~/bindings'

/*
 * Try it (src-tauri/src/tryit.rs): a request to the dev server on this machine for a route of the Routes view, the
 * answer beside the functions the route goes through. The server's address is kept per repository; a request can be
 * saved in the repository as a `.http` file (`http/`, the format REST Client and other tools read), and a saved one
 * for the same route fills the form. Only local addresses: nothing leaves the machine.
 */
const open = defineModel<boolean>('open', { default: false })
const props = defineProps<{ route: Route | null, handler: string | null }>()
const { repo } = useRepositories()
const { index, select } = useCode()
const failure = useFailure()
const toast = useToast()

const bases = usePersisted<Record<string, string>>('try-base', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
const base = computed({
  get: () => (repo.value ? bases.value[repo.value] : undefined) ?? 'http://localhost:3000',
  set: (value: string) => { if (repo.value) bases.value = { ...bases.value, [repo.value]: value.trim().replace(/\/+$/, '') } }
})

const method = ref('GET')
const path = ref('/')
const headers = ref('')
const body = ref('')
const response = shallowRef<HttpResponse | null>(null)
const sending = ref(false)
const saved = shallowRef<HttpFile[]>([])

/** A saved request's URL with the file's variables put in, and `{{baseUrl}}` as the address kept here. */
function resolved(file: HttpFile, url: string): string {
  return url.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, name: string) => file.variables.find(([key]) => key === name)?.[1] ?? whole)
}
const pathOf = (url: string) => url.replace(/^https?:\/\/[^/]+/, '') || '/'
/** A route's path, its placeholders kept for you to fill in: `/entries/:id`, WordPress's `(?P<id>\d+)` as `{id}`. */
const routePath = (route: Route) => route.path.replace(/\(\?P<(\w+)>[^)]*\)/g, '{$1}')

watch([open, () => props.route], async ([now, route]) => {
  if (!now || !route || !repo.value) return
  response.value = null
  method.value = route.method.split(',')[0]!.trim().replace('ALL', 'GET')
  path.value = routePath(route)
  const sends = ['POST', 'PUT', 'PATCH'].includes(method.value)
  headers.value = sends ? 'Content-Type: application/json' : ''
  body.value = sends ? '{\n  \n}' : ''
  try {
    saved.value = await haunt.httpFiles(repo.value)
  } catch {
    saved.value = []
  }
  // A saved request of the same method and path fills the form.
  for (const file of saved.value) {
    const match = file.requests.find(request => request.method === method.value && pathOf(resolved(file, request.url)).split('?')[0] === path.value)
    if (match) {
      path.value = pathOf(resolved(file, match.url))
      headers.value = match.headers.map(([name, value]) => `${name}: ${value}`).join('\n')
      body.value = match.body
      break
    }
  }
}, { immediate: true })

const request = computed<HttpRequest>(() => ({
  name: `${method.value} ${props.route?.path ?? path.value}`,
  method: method.value,
  url: `${base.value}${path.value.startsWith('/') ? '' : '/'}${path.value}`,
  headers: headers.value.split('\n').map(line => line.trim()).filter(Boolean).map((line) => {
    const at = line.indexOf(':')
    return [line.slice(0, at).trim(), line.slice(at + 1).trim()] as [string, string]
  }).filter(([name]) => name),
  body: body.value
}))

async function send(): Promise<void> {
  if (!repo.value || sending.value) return
  sending.value = true
  try {
    response.value = await haunt.sendRequest(repo.value, request.value)
  } catch (error) {
    response.value = null
    failure.failed('No answer', error, 'The request could not be sent.')
  } finally {
    sending.value = false
  }
}
const pretty = computed(() => {
  const text = response.value?.body ?? ''
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
})
const fileName = computed(() => {
  const words = (props.route?.path ?? path.value).split('/').filter(part => part && !part.startsWith(':') && !part.startsWith('{') && !part.startsWith('('))
  return `${words.slice(0, 2).join('-').replace(/[^\w-]+/g, '') || 'requests'}.http`
})
async function save(): Promise<void> {
  if (!repo.value) return
  try {
    const stored = { ...request.value, url: `{{baseUrl}}${path.value}` }
    const where = await haunt.saveRequest(repo.value, fileName.value, [['baseUrl', base.value]], stored)
    toast.add({ title: 'Request saved', description: where, icon: 'i-lucide-save', color: 'success' })
  } catch (error) {
    failure.failed('Not saved', error, 'The file could not be written.')
  }
}

/** The functions the route goes through: its handler and what that calls, two steps down. */
const through = computed(() => {
  const start = props.handler ? [props.handler] : props.route?.calls ?? []
  const out: { id: string, depth: number }[] = []
  const seen = new Set<string>()
  const walk = (id: string, depth: number) => {
    if (seen.has(id) || depth > 2 || out.length > 24 || !index.value?.functions.has(id)) return
    seen.add(id)
    out.push({ id, depth })
    for (const edge of index.value.callees.get(id) ?? []) walk(edge.to, depth + 1)
  }
  for (const id of start) walk(id, 0)
  return out
})
const name = (id: string) => index.value?.functions.get(id)?.fn.name ?? id
const statusColor = computed(() => {
  const status = response.value?.status ?? 0
  return status < 300 ? 'success' : status < 400 ? 'info' : status < 500 ? 'warning' : 'error'
})
</script>

<template>
  <USlideover v-model:open="open" :title="route ? `Try ${route.method} ${route.path}` : 'Try it'" description="A request to your dev server on this machine; nothing leaves it." :ui="{ content: 'max-w-4xl' }">
    <template #body>
      <div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_14rem]">
        <form class="grid content-start gap-3" @submit.prevent="send">
          <div class="flex gap-2">
            <UInput v-model="method" class="w-24" :ui="{ base: 'font-mono uppercase' }" aria-label="Method" />
            <UInput :model-value="base" class="w-56" :ui="{ base: 'font-mono' }" aria-label="Dev server address" title="Your dev server's address, kept for this repository" @update:model-value="(value: string) => base = value" />
            <UInput v-model="path" class="min-w-0 flex-1" :ui="{ base: 'font-mono' }" aria-label="Path" />
          </div>
          <UTextarea v-model="headers" :rows="2" autoresize placeholder="Header: value, one per line" :ui="{ base: 'font-mono text-xs' }" aria-label="Headers" />
          <UTextarea v-if="!['GET', 'HEAD'].includes(method.toUpperCase())" v-model="body" :rows="5" autoresize :maxrows="16" placeholder="Body" :ui="{ base: 'font-mono text-xs' }" aria-label="Body" />
          <div class="flex gap-2">
            <UButton type="submit" label="Send" icon="i-lucide-send" :loading="sending" />
            <UButton :label="`Save in http/${fileName}`" icon="i-lucide-save" color="neutral" variant="outline" title="Kept in the repository as a .http file, to commit with the code" @click="save" />
          </div>
          <section v-if="response" class="grid gap-2">
            <div class="flex items-center gap-2 text-sm">
              <UBadge :label="`${response.status} ${response.reason}`" :color="statusColor" variant="subtle" />
              <span class="text-xs text-muted tabular-nums">{{ response.ms }} ms</span>
              <span v-if="response.truncated" class="text-xs text-warning">Only the first 2 MB are shown.</span>
            </div>
            <details class="text-xs text-muted">
              <summary class="cursor-pointer">{{ response.headers.length }} headers</summary>
              <ul class="mt-1 grid gap-0.5 font-mono">
                <li v-for="([key, value], at) in response.headers" :key="at"><span class="text-default">{{ key }}:</span> {{ value }}</li>
              </ul>
            </details>
            <pre class="max-h-96 overflow-auto rounded border border-default bg-elevated/40 p-2 font-mono text-xs whitespace-pre-wrap">{{ pretty || '(no body)' }}</pre>
          </section>
        </form>
        <aside class="grid content-start gap-1">
          <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">Goes through</h3>
          <p v-if="!through.length" class="text-xs text-muted">Its handler is not a function the scan knows.</p>
          <button v-for="item in through" :key="item.id" type="button" class="truncate text-left font-mono text-xs hover:underline" :style="{ paddingLeft: `${item.depth * 12}px` }" :class="item.depth ? 'text-muted' : 'text-highlighted'" @click="select(item.id)">{{ name(item.id) }}</button>
          <p class="mt-2 text-label text-muted">From the code as written: the path a request takes may branch.</p>
        </aside>
      </div>
    </template>
  </USlideover>
</template>
