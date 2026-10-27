<script setup lang="ts">
// The folder's trust in the toolbar: how Haunt reads it, what runs when it is trusted, and a way to change it.
const { info: root, setTrust } = useRepositories()
const { trustDismissed: dismissed } = usePreferences()
const problem = ref('')
const reading = ref(false)

async function change(trusted: boolean) {
  problem.value = (await setTrust(trusted)) ?? ''
  // Asking again later shows the banner.
  if (!trusted && root.value?.root) dismissed.value = dismissed.value.filter(path => path !== root.value!.root)
}
function ask() {
  if (root.value?.root) dismissed.value = dismissed.value.filter(path => path !== root.value!.root)
}
</script>

<template>
  <UPopover v-if="root?.root" :content="{ align: 'end' }">
    <UButton
      :icon="root.trusted ? 'i-lucide-shield-check' : 'i-lucide-shield'"
      :aria-label="root.trusted ? t('TrustControl.trusted') : t('TrustControl.untrusted')"
      :title="root.trusted ? t('TrustControl.trusted') : t('TrustControl.untrusted')"
      size="sm"
      color="neutral"
      variant="ghost"
      :class="root.trusted ? 'text-success' : 'text-muted'"
    />
    <template #content>
      <div class="grid w-80 gap-2 p-3 text-sm">
        <template v-if="!root.wants.length">
          <p class="font-medium text-highlighted">{{ t('TrustControl.nothing') }}</p>
          <TrustReading class="text-muted" />
        </template>
        <template v-else>
          <p class="font-medium text-highlighted">{{ root.trusted ? t('TrustControl.runs') : t('TrustControl.reads') }}</p>
          <TrustReading v-if="!root.trusted" class="text-muted" />
          <p class="text-muted">{{ root.trusted ? t('TrustControl.itRuns') : t('TrustControl.wouldRun') }}</p>
          <ul class="grid gap-0.5 text-muted">
            <li v-for="want in root.wants" :key="want.path">{{ t('TrustControl.its', { label: said(want.label) }) }} <code class="text-xs">{{ want.path }}</code></li>
          </ul>
          <UButton :label="t('TrustControl.see')" size="xs" color="neutral" variant="link" class="justify-self-start px-0" @click="reading = true" />
          <p v-if="problem" class="text-xs text-error">{{ problem }}</p>
          <UButton v-if="root.trusted" :label="t('TrustControl.stop')" size="sm" color="neutral" variant="outline" class="justify-self-start" @click="change(false)" />
          <UButton v-else :label="t('TrustControl.ask')" size="sm" color="neutral" variant="outline" class="justify-self-start" @click="ask" />
        </template>
      </div>
    </template>
  </UPopover>
  <TrustDetails v-model:open="reading" />
</template>
