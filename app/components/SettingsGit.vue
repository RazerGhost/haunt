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
const pulls: { value: PullChoice, label: string, help: string }[] = [
  { value: 'ask', label: 'Ask', help: 'Haunt asks each time whether to rebase or merge.' },
  { value: 'rebase', label: 'Rebase', help: 'Your commits go on top of the remote ones, in a straight line.' },
  { value: 'merge', label: 'Merge', help: 'A merge commit joins the two.' }
]
const intervals = [
  { value: 0, label: 'Off' },
  { value: 5, label: 'Every 5 minutes' },
  { value: 15, label: 'Every 15 minutes' },
  { value: 30, label: 'Every 30 minutes' }
]
</script>

<template>
  <div class="grid content-start gap-5">
    <template v-if="app">
      <UFormField label="When branches diverge" :help="pulls.find(item => item.value === app!.pull)?.help">
        <UFieldGroup>
          <UButton
            v-for="option in pulls"
            :key="option.value"
            :label="option.label"
            size="sm"
            :color="app.pull === option.value ? 'primary' : 'neutral'"
            :variant="app.pull === option.value ? 'solid' : 'outline'"
            @click="saveGit(option.value, app!.fetchEvery)"
          />
        </UFieldGroup>
      </UFormField>
      <UFormField label="Fetch in the background" help="Keeps the Pull and Push counts current. Remotes that need a sign-in are skipped.">
        <USelect :model-value="app.fetchEvery" :items="intervals" class="w-56" @update:model-value="value => saveGit(app!.pull, Number(value))" />
      </UFormField>
      <USwitch
        :model-value="app.hostTools"
        label="Pull request status"
        description="Shows each branch's pull request and checks via gh, glab or tea (Gitea untested). Uses your account; runs nothing from the repository."
        @update:model-value="(value: boolean) => saveOnline(value, app!.updateCheck)"
      />
    </template>
    <USwitch v-model="signoff" label="Sign off by default" description="Sign off is ticked in the commit box (adds a Signed-off-by line)." />
  </div>
</template>
