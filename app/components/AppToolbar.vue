<script setup lang="ts">
/*
 * The row under the tabs: Git or Code (the two halves of the window, useMode), the git actions (fetch, pull and push with git's progress while they run, and a way to stop;
 * branch and stash), the branch, the search, and the scan, trust and settings icons.
 */
const { scan, search } = useCode()
const { current, refs } = useRefs()
const { mac } = useDesktop()
const refActions = useRefActions()
const network = useNetwork()
const { mode, switchTo } = useMode()

const branch = computed(() => refs.value ? (refs.value.branch ?? refs.value.sha?.slice(0, 7) ?? 'no commits') : null)
const branchTip = computed(() => {
  const at = current.value
  if (!refs.value) return ''
  if (!at) return refs.value.sha ? `Detached at ${refs.value.sha.slice(0, 7)}` : 'No commits yet'
  if (!at.upstream) return `${at.name} tracks no remote branch`
  if (at.gone) return `${at.name} tracked ${at.upstream}, which is gone`
  return `${at.name} · ${at.ahead} to push, ${at.behind} to pull (${at.upstream})`
})

// Pull and push run only in a trusted folder (they run its hooks and filters); fetch runs anywhere.
const untrusted = "Runs only in a trusted folder, since it runs the repository's hooks and filters"
const actions = computed(() => {
  const at = current.value
  const upstream = at?.upstream
  return [
    { label: 'Fetch', op: 'fetch', icon: 'i-lucide-refresh-cw', count: 0, run: () => network.fetch(), tip: `Fetch every remote: new commits and remote branches; your branches and files stay as they are (F5).${fetchedTip.value}` },
    {
      label: 'Pull', op: 'pull', icon: 'i-lucide-arrow-down-to-line', count: at?.behind ?? 0,
      run: network.trusted.value && upstream ? () => network.pull() : null,
      tip: !network.trusted.value ? untrusted : upstream ? `Pull ${upstream} into ${at!.name} (${mac ? '⌘⇧P' : 'Ctrl+Shift+P'})` : 'This branch tracks no remote branch'
    },
    {
      label: 'Push', op: 'push', icon: 'i-lucide-arrow-up-from-line', count: at?.ahead ?? 0,
      run: network.trusted.value && at ? () => network.push() : null,
      tip: !network.trusted.value ? untrusted : !at ? 'You are not on a branch' : `${upstream ? `Push ${at.name} to ${upstream}` : `Push ${at.name} and track it on the remote`} (${mac ? '⌘⇧U' : 'Ctrl+Shift+U'})`
    },
    { label: 'Branch', op: null, icon: 'i-lucide-git-branch-plus', count: 0, run: () => refActions.newBranch(), tip: 'A new branch from the commit you are on' },
    { label: 'Stash', op: null, icon: 'i-lucide-archive', count: 0, run: () => refActions.stash(), tip: 'Stash your local changes' }
  ]
})
const busy = computed(() => refActions.busy.value || !!network.running.value)
/** When it last fetched, for Fetch's tooltip; a failed fetch also marks the button. */
const now = ref(new Date())
let ticking: ReturnType<typeof setInterval> | undefined
onMounted(() => { ticking = setInterval(() => { now.value = new Date() }, 30_000) })
onBeforeUnmount(() => clearInterval(ticking))
const fetchedTip = computed(() => {
  const last = network.lastFetch.value
  if (!last) return ''
  const minutes = Math.max(0, Math.round((now.value.getTime() / 1000 - last.at) / 60))
  const ago = minutes < 1 ? 'just now' : minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`
  return last.ok ? ` Fetched ${ago}${last.background ? ' (in the background)' : ''}.` : ` The last fetch failed ${ago}: ${last.message ?? 'git did not say why'}`
})
const fetchFailed = computed(() => network.lastFetch.value?.ok === false)

const searchInput = useTemplateRef<{ inputRef: HTMLInputElement | null }>('searchInput')
// Ctrl+K opens the command palette (AppPalette); Ctrl+F finds a function in the code views.
const router = useRouter()
defineShortcuts({
  meta_f: () => searchInput.value?.inputRef?.focus(),
  // F5 fetches, Ctrl+Shift+P pulls and Ctrl+Shift+U pushes (with their questions); Alt+← and Alt+→ go back and forward.
  f5: () => { if (!busy.value) void network.fetch() },
  meta_shift_p: () => { if (!busy.value && network.trusted.value && current.value?.upstream) void network.pull() },
  meta_shift_u: () => { if (!busy.value && network.trusted.value && current.value) void network.push() },
  alt_arrowleft: () => router.back(),
  alt_arrowright: () => router.forward(),
  // Ctrl+1 for Git, Ctrl+2 for Code (⌘ on macOS).
  meta_1: () => switchTo('git'),
  meta_2: () => switchTo('code')
})
</script>

<template>
  <div class="flex h-11 shrink-0 items-center gap-1 border-b border-default px-2">
    <UFieldGroup size="sm" class="mr-1" aria-label="Git or code">
      <UTooltip :text="`Git: history, branches and changes (${mac ? '⌘1' : 'Ctrl+1'})`">
        <UButton label="Git" icon="i-lucide-git-branch" :color="mode === 'git' ? 'primary' : 'neutral'" :variant="mode === 'git' ? 'solid' : 'outline'" @click="switchTo('git')" />
      </UTooltip>
      <UTooltip :text="`Code mode (${mac ? '⌘2' : 'Ctrl+2'})`">
        <UButton label="Code" icon="i-lucide-braces" :color="mode === 'code' ? 'primary' : 'neutral'" :variant="mode === 'code' ? 'solid' : 'outline'" @click="switchTo('code')" />
      </UTooltip>
    </UFieldGroup>
    <div class="flex items-center gap-0.5" role="toolbar" aria-label="Git">
      <UTooltip v-for="action in actions" :key="action.label" :text="action.tip">
        <span>
          <UButton
            :icon="network.running.value?.op === action.op ? 'i-lucide-loader-circle' : action.icon"
            :label="action.label"
            color="neutral"
            variant="ghost"
            size="sm"
            :disabled="!action.run || busy"
            class="relative"
            :ui="{ label: 'hidden lg:inline', leadingIcon: network.running.value?.op === action.op ? 'animate-spin' : '' }"
            @click="action.run?.()"
          >
            <template v-if="action.op === 'fetch' && fetchFailed" #trailing>
              <span class="size-2 rounded-full bg-warning" aria-label="The last fetch failed" />
            </template>
            <template v-else-if="action.count" #trailing>
              <span class="rounded-full bg-elevated px-1.5 text-label font-semibold text-highlighted tabular-nums">{{ action.count }}</span>
            </template>
          </UButton>
        </span>
      </UTooltip>
    </div>

    <UTooltip v-if="current?.behind && !network.running.value" text="What a pull would change, and where you changed the same">
      <UButton icon="i-lucide-eye" label="Preview" size="xs" color="neutral" variant="soft" to="/incoming" :ui="{ label: 'hidden xl:inline' }" />
    </UTooltip>
    <div v-if="network.running.value" class="flex min-w-0 items-center gap-1 pl-1 text-ui text-muted" role="status">
      <span class="max-w-72 truncate tabular-nums">{{ network.running.value.text || 'Connecting…' }}</span>
      <UTooltip text="Stop git">
        <UButton icon="i-lucide-square" size="xs" color="neutral" variant="ghost" aria-label="Stop" @click="network.stop()" />
      </UTooltip>
    </div>

    <USeparator orientation="vertical" class="mx-1 h-5" />

    <UTooltip v-if="branch" :text="branchTip">
      <UButton to="/local" color="neutral" variant="ghost" size="sm" icon="i-lucide-git-branch" class="max-w-56 min-w-0 font-mono text-xs" :label="branch" :ui="{ label: 'truncate' }" />
    </UTooltip>

    <div class="min-w-2 flex-1" />

    <UInput
      v-if="scan"
      ref="searchInput"
      v-model="search"
      icon="i-lucide-search"
      placeholder="Find a function"
      size="sm"
      class="w-48 lg:w-64"
      :ui="{ base: 'font-mono', trailing: 'pe-1.5' }"
      aria-label="Find a function"
    >
      <template #trailing>
        <UKbd v-if="!search" :value="mac ? '⌘F' : 'Ctrl F'" size="sm" />
        <UButton v-else icon="i-lucide-x" size="xs" color="neutral" variant="link" aria-label="Clear the search" @click="search = ''" />
      </template>
    </UInput>

    <div class="flex items-center gap-0.5">
      <ScanStatus />
      <TrustControl />
      <SettingsMenu />
    </div>
  </div>
</template>
