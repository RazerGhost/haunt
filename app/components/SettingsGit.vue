<script setup lang="ts">
import type { PullChoice } from '~/bindings'

/*
 * Settings, Git: what a pull does when the branches went apart, the background fetch and the host's tool (kept by Rust,
 * which acts on them), and sign-off by default (kept in the window).
 */
const { signoff } = usePreferences()
const { app, save, saveOnline } = useAppSettings()

async function saveGit(pull: PullChoice, fetchEvery: number): Promise<void> {
  await save(haunt.setGitSettings(pull, fetchEvery))
}
const pulls: PullChoice[] = ['ask', 'rebase', 'merge']
const intervals = computed(() => [0, 5, 15, 30].map(value => ({ value, label: value ? t('SettingsGit.every', { count: value }) : t('SettingsGit.off') })))
</script>

<template>
  <div class="grid content-start gap-5">
    <template v-if="app">
      <UFormField :label="t('SettingsGit.diverge')" :help="t(`SettingsGit.pull.${app.pull}Help`)">
        <UFieldGroup>
          <UButton
            v-for="option in pulls"
            :key="option"
            :label="t(`SettingsGit.pull.${option}`)"
            size="sm"
            :color="app.pull === option ? 'primary' : 'neutral'"
            :variant="app.pull === option ? 'solid' : 'outline'"
            @click="saveGit(option, app!.fetchEvery)"
          />
        </UFieldGroup>
      </UFormField>
      <UFormField :label="t('SettingsGit.fetch')" :help="t('SettingsGit.fetchHelp')">
        <USelect :model-value="app.fetchEvery" :items="intervals" class="w-56" @update:model-value="value => saveGit(app!.pull, Number(value))" />
      </UFormField>
      <USwitch
        :model-value="app.hostTools"
        :label="t('SettingsGit.hostTools')"
        :description="t('SettingsGit.hostToolsHelp')"
        @update:model-value="(value: boolean) => saveOnline(value, app!.updateCheck)"
      />
    </template>
    <USwitch v-model="signoff" :label="t('SettingsGit.signoff')" :description="t('SettingsGit.signoffHelp')" />
  </div>
</template>
