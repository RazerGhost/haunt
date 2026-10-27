<script setup lang="ts">
/*
 * Said where layers are drawn: the folder names its own layers in an architecture script, which runs only once the
 * folder is trusted, so until then the map shows Haunt's own grouping (workspaces and folders) instead.
 */
const { info, setTrust } = useRepositories()
const { scan } = useCode()
const script = computed(() => info.value?.wants.find(want => want.kind === 'script') ?? null)
const shown = computed(() => !!script.value && !!scan.value && !scan.value.trusted && !scan.value.rules)
const problem = ref('')
const busy = ref(false)
async function trust(): Promise<void> {
  busy.value = true
  problem.value = (await setTrust(true)) ?? ''
  busy.value = false
}
</script>

<template>
  <UAlert
    v-if="shown"
    color="neutral"
    variant="subtle"
    icon="i-lucide-shield"
    :title="t('TrustLayersNote.title')"
    :description="problem || t('TrustLayersNote.description', { path: script!.path })"
  >
    <template #actions>
      <UButton :label="t('TrustLayersNote.trust')" icon="i-lucide-shield-check" size="xs" :loading="busy" @click="trust" />
    </template>
  </UAlert>
</template>
