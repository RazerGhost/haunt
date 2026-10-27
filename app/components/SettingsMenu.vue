<script setup lang="ts">
/*
 * Settings, from the gear (or Ctrl+, / ⌘,), in five sections, each its own component: General (SettingsGeneral),
 * Appearance (SettingsAppearance), Git (SettingsGit), Trusted folders (SettingsTrust) and About (SettingsAbout). The
 * settings Rust keeps are read each time the window opens (useAppSettings).
 */
const { problem, load } = useAppSettings()

const open = ref(false)
type Section = 'general' | 'appearance' | 'git' | 'trust' | 'about'
const section = ref<Section>('general')
const sections: { value: Section, icon: string }[] = [
  { value: 'general', icon: 'i-lucide-sliders-horizontal' },
  { value: 'appearance', icon: 'i-lucide-palette' },
  { value: 'git', icon: 'i-lucide-git-branch' },
  { value: 'trust', icon: 'i-lucide-shield-check' },
  { value: 'about', icon: 'i-lucide-info' }
]
defineShortcuts({ 'meta_,': () => { open.value = true } })
watch(open, (now) => {
  if (now) void load()
})
</script>

<template>
  <UTooltip :text="t('common.settings')" :kbds="['meta', ',']">
    <UButton icon="i-lucide-settings-2" color="neutral" variant="ghost" size="sm" :aria-label="t('common.settings')" @click="open = true" />
  </UTooltip>
  <UModal v-model:open="open" :title="t('common.settings')" :ui="{ content: 'max-w-3xl', body: 'p-0 sm:p-0' }">
    <template #body>
      <div class="grid min-h-[26rem] grid-cols-[12rem_minmax(0,1fr)]">
        <nav class="grid content-start gap-0.5 border-r border-default p-2" :aria-label="t('SettingsMenu.sections')">
          <button
            v-for="item in sections"
            :key="item.value"
            type="button"
            class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-ui"
            :class="section === item.value ? 'bg-elevated font-medium text-highlighted' : 'text-default hover:bg-elevated/60'"
            :aria-current="section === item.value ? 'page' : undefined"
            @click="section = item.value"
          >
            <UIcon :name="item.icon" class="size-4 shrink-0 text-muted" />{{ t(`SettingsMenu.${item.value}`) }}
          </button>
        </nav>

        <div class="grid content-start gap-5 p-5 text-sm">
          <UAlert v-if="problem && section !== 'general'" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
          <SettingsGeneral v-if="section === 'general'" />
          <SettingsAppearance v-else-if="section === 'appearance'" />
          <SettingsGit v-else-if="section === 'git'" />
          <SettingsTrust v-else-if="section === 'trust'" />
          <SettingsAbout v-else />
        </div>
      </div>
    </template>
  </UModal>
</template>
