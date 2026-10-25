<script setup lang="ts">
import type { AppFolder } from '~/bindings'

/* Settings, About: the version, the license, looking for updates, and where Haunt keeps its files. */
const { app, saveOnline } = useAppSettings()
const toast = useToast()

const updates = useUpdates()
async function checkNow(): Promise<void> {
  if (app.value) await updates.check(app.value.version)
}

const folders = computed(() => app.value
  ? [
      { which: 'settings' as AppFolder, label: 'Settings', path: app.value.folders.settings, none: '' },
      { which: 'cache' as AppFolder, label: 'Cache', path: app.value.folders.cache, none: 'None here.' },
      { which: 'logs' as AppFolder, label: 'Logs', path: app.value.folders.logs, none: 'While Haunt is worked on, the scanner writes to the console instead.' }
    ]
  : [])
async function reveal(which: AppFolder): Promise<void> {
  try {
    await haunt.openFolder(which)
  } catch (error) {
    toast.add({ title: 'Could not open the folder', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function clearCache(): Promise<void> {
  try {
    const removed = await haunt.clearCallGraphs()
    toast.add({ title: removed ? `Removed ${removed} kept call ${removed === 1 ? 'graph' : 'graphs'}` : 'There were no kept call graphs', description: removed ? 'Each repository works its call graph out again on its next scan.' : undefined, icon: 'i-lucide-check' })
  } catch (error) {
    toast.add({ title: 'Not cleared', description: errorMessage(error, 'Try again.'), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
</script>

<template>
  <div class="grid content-start gap-5">
    <dl v-if="app" class="grid grid-cols-[8rem_minmax(0,1fr)] gap-x-4 gap-y-2">
      <dt class="text-muted">Version</dt>
      <dd class="font-mono">{{ app.version }}</dd>
      <dt class="text-muted">License</dt>
      <dd>Apache License 2.0</dd>
    </dl>
    <div v-if="app" class="grid gap-2">
      <USwitch
        :model-value="app.updateCheck"
        label="Check for updates"
        description="Once a day at most, Haunt asks GitHub for its latest release. Nothing else is sent."
        @update:model-value="(value: boolean) => saveOnline(app!.hostTools, value)"
      />
      <div class="flex flex-wrap items-center gap-2">
        <UButton label="Check for updates" icon="i-lucide-refresh-cw" size="xs" color="neutral" variant="outline" :loading="updates.checking.value" @click="checkNow" />
        <span v-if="updates.result.value?.problem" class="text-xs text-muted">{{ updates.result.value.problem }}</span>
        <template v-else-if="updates.result.value">
          <span v-if="updates.result.value.newer" class="text-xs text-highlighted">Haunt {{ updates.result.value.latest }} is out.</span>
          <span v-else class="text-xs text-muted">This is the latest version ({{ updates.result.value.latest }}).</span>
          <UButton v-if="updates.result.value.newer && updates.result.value.url" label="See it" size="xs" color="neutral" variant="link" @click="updates.open(updates.result.value.url!)" />
        </template>
      </div>
    </div>
    <div v-if="app" class="grid gap-3">
      <div v-for="folder in folders" :key="folder.which" class="grid gap-1">
        <span class="text-muted">{{ folder.label }}</span>
        <div v-if="folder.path" class="flex items-center gap-2">
          <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="folder.path">{{ folder.path }}</span>
          <UButton label="Open" icon="i-lucide-folder-open" size="xs" color="neutral" variant="ghost" @click="reveal(folder.which)" />
        </div>
        <span v-else class="text-xs text-dimmed">{{ folder.none }}</span>
      </div>
    </div>
    <div>
      <UButton label="Clear cached call graphs" icon="i-lucide-eraser" size="sm" color="neutral" variant="outline" @click="clearCache" />
      <p class="mt-1 text-xs text-muted">Call graphs are kept so repositories open at once. Clearing frees space; they are worked out again on the next scan.</p>
    </div>
  </div>
</template>
