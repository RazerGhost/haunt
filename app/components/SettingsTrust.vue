<script setup lang="ts">
/*
 * Settings, Trusted folders: every folder whose own code Haunt runs, each with a way to stop. A folder that is missing,
 * or that is another folder now (made after it was trusted, so not trusted), is marked; those can be forgotten at once.
 */
const { app, save } = useAppSettings()
const stale = computed(() => app.value?.trusted.filter(folder => folder.missing || folder.replaced).length ?? 0)

async function stopTrusting(path: string): Promise<void> {
  await save(haunt.forgetTrust(path), t('SettingsTrust.stillTrusted'))
}

async function forgetStale(): Promise<void> {
  await save(haunt.forgetStaleTrust(), t('SettingsTrust.stillTrusted'))
}
</script>

<template>
  <div class="grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-5">
    <p class="text-muted">{{ t('SettingsTrust.intro') }}</p>
    <p v-if="app && !app.trusted.length" class="text-muted">{{ t('SettingsTrust.none') }}</p>
    <ul v-else-if="app" class="grid grid-cols-[minmax(0,1fr)] gap-1">
      <li v-for="folder in app.trusted" :key="folder.path" class="flex items-center gap-2 rounded-md border border-default px-3 py-2">
        <UIcon name="i-lucide-folder" class="size-4 shrink-0 text-muted" />
        <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="folder.path">{{ folder.path }}</span>
        <UBadge v-if="folder.missing" :label="t('SettingsTrust.notFound')" color="neutral" variant="subtle" size="sm" />
        <UBadge v-else-if="folder.replaced" :label="t('SettingsTrust.replaced')" :title="t('SettingsTrust.replacedHelp')" color="warning" variant="subtle" size="sm" />
        <UButton :label="t('SettingsTrust.stop')" size="xs" color="neutral" variant="outline" @click="stopTrusting(folder.path)" />
      </li>
    </ul>
    <div v-if="stale" class="flex flex-wrap items-center gap-x-3 gap-y-1">
      <UButton icon="i-lucide-eraser" :label="t('SettingsTrust.forgetStale', { count: stale })" size="sm" color="neutral" variant="outline" @click="forgetStale" />
      <span class="text-xs text-muted">{{ t('SettingsTrust.forgetStaleHelp') }}</span>
    </div>
  </div>
</template>
