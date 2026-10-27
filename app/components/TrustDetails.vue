<script setup lang="ts">
import type { WantedText } from '~/bindings'

/*
 * What a folder would run once trusted, to read before trusting it (src-tauri/src/repos.rs, `wanted_texts`): its git
 * hooks (and the Husky files they hand over to), its architecture script, and its own TypeScript. From the trust
 * banner and the shield in the toolbar.
 */
const open = defineModel<boolean>('open', { default: false })
const { repo, info, setTrust } = useRepositories()
const texts = shallowRef<WantedText[]>([])
const loading = ref(false)
const problem = ref('')

watch(open, async (now) => {
  if (!now || !repo.value) return
  loading.value = true
  problem.value = ''
  try {
    texts.value = await haunt.wantedTexts(repo.value)
  } catch {
    texts.value = []
  } finally {
    loading.value = false
  }
})
// A hook has no extension: its first line says what reads it (#!/bin/sh), so it is coloured as a shell script.
const pathFor = (item: WantedText) => (/\.[a-z]+$/i.test(item.path) ? item.path : `${item.path}.sh`)

async function trust(): Promise<void> {
  problem.value = (await setTrust(true)) ?? ''
  if (!problem.value) open.value = false
}
</script>

<template>
  <UModal v-model:open="open" :ui="{ content: 'max-w-3xl' }" :title="t('TrustDetails.title')" :description="t('TrustDetails.description')">
    <template #body>
      <p v-if="loading" class="text-sm text-muted">{{ t('TrustDetails.reading') }}</p>
      <p v-else-if="!texts.length" class="text-sm text-muted">{{ t('TrustDetails.nothing') }}</p>
      <div v-else class="grid gap-4">
        <section v-for="item in texts" :key="item.path" class="grid gap-1.5">
          <div class="flex items-baseline gap-2">
            <h3 class="text-sm font-medium text-highlighted">{{ t('TrustDetails.its', { label: said(item.label) }) }}</h3>
            <code class="min-w-0 truncate text-xs text-muted">{{ item.path }}</code>
          </div>
          <p class="text-xs text-muted">{{ said(item.note) }}</p>
          <div v-if="item.text" class="max-h-72 overflow-auto rounded border border-default bg-elevated/40 p-2">
            <CodeLines :path="pathFor(item)" :text="item.text" />
          </div>
          <p v-if="item.truncated" class="text-xs text-warning">{{ t('TrustDetails.truncated') }}</p>
        </section>
      </div>
    </template>
    <template #footer>
      <div class="flex w-full items-center gap-2">
        <p v-if="problem" class="flex-1 text-xs text-error">{{ problem }}</p>
        <span v-else class="flex-1" />
        <UButton :label="t('common.close')" color="neutral" variant="ghost" @click="open = false" />
        <UButton v-if="info && !info.trusted" :label="t('TrustDetails.trust')" icon="i-lucide-shield-check" :disabled="loading" @click="trust" />
      </div>
    </template>
  </UModal>
</template>
