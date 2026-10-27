<script setup lang="ts">
/* Settings, Trusted folders: every folder whose own code Haunt runs, each with a way to stop. */
const { app, save } = useAppSettings()

async function stopTrusting(path: string): Promise<void> {
  await save(haunt.forgetTrust(path), t('SettingsTrust.stillTrusted'))
}
</script>

<template>
  <div class="grid content-start gap-5">
    <p class="text-muted">{{ t('SettingsTrust.intro') }}</p>
    <p v-if="app && !app.trusted.length" class="text-muted">{{ t('SettingsTrust.none') }}</p>
    <ul v-else-if="app" class="grid gap-1">
      <li v-for="folder in app.trusted" :key="folder.path" class="flex items-center gap-2 rounded-md border border-default px-3 py-2">
        <UIcon name="i-lucide-folder" class="size-4 shrink-0 text-muted" />
        <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="folder.path">{{ folder.path }}</span>
        <UBadge v-if="folder.missing" :label="t('SettingsTrust.notFound')" color="neutral" variant="subtle" size="sm" />
        <UButton :label="t('SettingsTrust.stop')" size="xs" color="neutral" variant="outline" @click="stopTrusting(folder.path)" />
      </li>
    </ul>
  </div>
</template>
