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
      { which: 'settings' as AppFolder, label: t('common.settings'), path: app.value.folders.settings, none: '' },
      { which: 'cache' as AppFolder, label: t('SettingsAbout.cache'), path: app.value.folders.cache, none: t('SettingsAbout.noCache') },
      { which: 'logs' as AppFolder, label: t('SettingsAbout.logs'), path: app.value.folders.logs, none: t('SettingsAbout.noLogs') }
    ]
  : [])
async function reveal(which: AppFolder): Promise<void> {
  try {
    await haunt.openFolder(which)
  } catch (error) {
    toast.add({ title: t('SettingsAbout.folderNotOpened'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
async function clearCache(): Promise<void> {
  try {
    const removed = await haunt.clearCallGraphs()
    toast.add({ title: removed ? t('SettingsAbout.cleared', { count: removed }) : t('SettingsAbout.nothingToClear'), description: removed ? t('SettingsAbout.clearedHelp') : undefined, icon: 'i-lucide-check' })
  } catch (error) {
    toast.add({ title: t('SettingsAbout.notCleared'), description: errorMessage(error, t('common.tryAgain')), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
</script>

<template>
  <div class="grid content-start gap-5">
    <dl v-if="app" class="grid grid-cols-[8rem_minmax(0,1fr)] gap-x-4 gap-y-2">
      <dt class="text-muted">{{ t('SettingsAbout.version') }}</dt>
      <dd class="font-mono">{{ app.version }}</dd>
      <dt class="text-muted">{{ t('SettingsAbout.license') }}</dt>
      <dd>Apache License 2.0</dd>
    </dl>
    <div v-if="app" class="grid gap-2">
      <USwitch
        :model-value="app.updateCheck"
        :label="t('SettingsAbout.updateCheck')"
        :description="t('SettingsAbout.updateCheckHelp')"
        @update:model-value="(value: boolean) => saveOnline(app!.hostTools, value)"
      />
      <div class="flex flex-wrap items-center gap-2">
        <UButton :label="t('SettingsAbout.checkNow')" icon="i-lucide-refresh-cw" size="xs" color="neutral" variant="outline" :loading="updates.checking.value" @click="checkNow" />
        <span v-if="updates.result.value?.problem" class="text-xs text-muted">{{ updates.result.value.problem }}</span>
        <template v-else-if="updates.result.value">
          <span v-if="updates.result.value.newer" class="text-xs text-highlighted">{{ t('SettingsAbout.newer', { version: updates.result.value.latest ?? '' }) }}</span>
          <span v-else class="text-xs text-muted">{{ t('SettingsAbout.latest', { version: updates.result.value.latest ?? '' }) }}</span>
          <UButton v-if="updates.result.value.newer && updates.result.value.url" :label="t('SettingsAbout.seeIt')" size="xs" color="neutral" variant="link" @click="updates.open(updates.result.value.url!)" />
        </template>
      </div>
    </div>
    <div v-if="app" class="grid gap-3">
      <div v-for="folder in folders" :key="folder.which" class="grid gap-1">
        <span class="text-muted">{{ folder.label }}</span>
        <div v-if="folder.path" class="flex items-center gap-2">
          <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="folder.path">{{ folder.path }}</span>
          <UButton :label="t('common.open')" icon="i-lucide-folder-open" size="xs" color="neutral" variant="ghost" @click="reveal(folder.which)" />
        </div>
        <span v-else class="text-xs text-dimmed">{{ folder.none }}</span>
      </div>
    </div>
    <div>
      <UButton :label="t('SettingsAbout.clear')" icon="i-lucide-eraser" size="sm" color="neutral" variant="outline" @click="clearCache" />
      <p class="mt-1 text-xs text-muted">{{ t('SettingsAbout.clearHelp') }}</p>
    </div>
  </div>
</template>
