<script setup lang="ts">
import { en, nl } from '@nuxt/ui/locale'

/*
 * The window: the tabs on top, the toolbar under them, the sidebar on the left, the screen in the middle and the
 * selected function on the right. Each tab remembers its screen.
 */
const { selected, select, status } = useCode()
const { picking, problems, rememberRoute } = useRepositories()
const route = useRoute()
const choosing = computed(() => picking.value || status.value === 'no-root')
const isWide = ref(true)
onMounted(() => {
  const query = window.matchMedia('(min-width: 1024px)')
  isWide.value = query.matches
  query.addEventListener('change', event => { isWide.value = event.matches })
})
const panelOpen = computed({
  get: () => !!selected.value && !isWide.value,
  set: (open) => { if (!open) select(null) }
})
watch(() => route.fullPath, () => rememberRoute())
// The text size from Settings: the root font size, which everything in rem follows; a step either side of the
// profile's theme's own size (useAppearance).
const { textSize } = usePreferences()
const { baseSize } = useAppearance()
if (import.meta.client) watch([textSize, baseSize], ([size, base]) => { document.documentElement.style.fontSize = `${base + TEXT_STEPS[size]}px` }, { immediate: true })

// The window's language (Settings, General), for its texts (t), Nuxt UI's own and the page's lang.
const language = useLanguage()
const uiLocales = { en, nl }
useHead({ title: 'Haunt', htmlAttrs: { lang: language }, link: [{ rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }] })
</script>

<template>
  <UApp :locale="uiLocales[language]">
    <div class="flex h-dvh flex-col">
      <TitleBar />
      <UAlert v-if="problems.length" color="error" variant="subtle" icon="i-lucide-circle-alert" :title="t('app.missing')" class="rounded-none">
        <template #description>
          <p v-for="problem in problems" :key="problem">{{ said(problem) }}</p>
        </template>
      </UAlert>
      <AppToolbar v-if="!choosing" />
      <TrustBanner v-if="!choosing" />
      <OperationBanner v-if="!choosing" />
      <BisectBanner v-if="!choosing" />
      <div class="flex min-h-0 flex-1">
        <AppSidebar v-if="!choosing" />
        <main class="min-w-0 flex-1 overflow-auto">
          <RepositoryManager v-if="choosing" />
          <NuxtPage v-else />
        </main>
        <aside v-if="selected && isWide && !choosing" class="w-96 shrink-0 overflow-auto border-l border-default">
          <FunctionPanel />
        </aside>
      </div>
    </div>
    <USlideover v-model:open="panelOpen" :title="t('common.function')">
      <template #body>
        <FunctionPanel />
      </template>
    </USlideover>
    <AppDialog />
    <AppPalette />
    <ShortcutsSheet />
  </UApp>
</template>
