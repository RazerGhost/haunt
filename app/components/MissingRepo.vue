<script setup lang="ts">
/*
 * A repository whose folder is missing (missing.rs): moved, renamed, deleted, or on a drive that is not connected. Shown
 * in place of the screen when the shown repository's folder goes, or a missing tab or recent repository is clicked.
 * Locate… points Haunt at where the folder is now (its tab, look, trust, recent entry and what the window keeps of it
 * move there); Remove takes its tab and recent entry away, with what the window keeps of it. Left as it is, it opens
 * again by itself once its folder is back.
 */
const props = defineProps<{ path: string }>()
const { info, picking, relocate, forgetMissing } = useRepositories()
const { inApp } = useDesktop()
const dialog = useDialog()

const name = computed(() => props.path.split(/[\\/]/).filter(Boolean).pop() ?? props.path)
const typed = ref('')
const problem = ref('')
const busy = ref(false)
const inTab = computed(() => info.value?.all_tabs.includes(props.path) ?? false)

async function locateAt(folder: string): Promise<void> {
  if (!folder.trim()) return
  busy.value = true
  problem.value = (await relocate(props.path, folder.trim())) ?? ''
  busy.value = false
}

async function locate(): Promise<void> {
  if (!inApp) return locateAt(typed.value)
  const { open } = await import('@tauri-apps/plugin-dialog')
  const chosen = await open({ directory: true, title: t('MissingRepo.locateTitle', { name: name.value }) })
  if (typeof chosen === 'string') await locateAt(chosen)
}

async function remove(): Promise<void> {
  const ok = await dialog.confirm({
    title: t('MissingRepo.removeTitle', { name: name.value }),
    description: t('MissingRepo.removeHelp'),
    confirm: t('MissingRepo.remove'),
    danger: true
  })
  if (ok) await forgetMissing(props.path)
}
</script>

<template>
  <div class="grid h-full place-items-center p-8">
    <div class="grid max-w-lg gap-4">
      <div class="flex items-center gap-3">
        <UIcon name="i-lucide-folder-x" class="size-8 shrink-0 text-warning" />
        <div class="grid min-w-0 gap-0.5">
          <h1 class="text-lg font-semibold text-highlighted">{{ t('MissingRepo.title', { name }) }}</h1>
          <p class="truncate font-mono text-label text-muted" :title="path">{{ path }}</p>
        </div>
      </div>
      <p class="text-sm text-default">{{ t('MissingRepo.help') }}</p>
      <p class="text-sm text-muted">{{ t('MissingRepo.waits') }}</p>
      <UInput v-if="!inApp" v-model="typed" :placeholder="t('MissingRepo.typedPlaceholder')" size="sm" @keydown.enter="locate" />
      <div class="flex flex-wrap gap-2">
        <UButton icon="i-lucide-folder-search" :label="t('MissingRepo.locate')" :loading="busy" @click="locate" />
        <UButton icon="i-lucide-trash-2" :label="inTab ? t('MissingRepo.removeTab') : t('MissingRepo.removeRecent')" color="neutral" variant="outline" @click="remove" />
        <UButton :label="t('MissingRepo.back')" color="neutral" variant="ghost" @click="picking = true" />
      </div>
      <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" />
    </div>
  </div>
</template>
