<script setup lang="ts">
/*
 * Local changes: as files, to stage, discard and commit (LocalChanges), or by function, with what they could break
 * (ChangesByFunction). The activity list shows every git command Haunt ran to write.
 */
const { notRepository, activity } = useLocalChanges()
const view = usePersisted<'files' | 'functions'>('local-view', 'files', value => value === 'files' || value === 'functions')
const when = (seconds: number) => new Date(seconds * 1000).toLocaleTimeString()
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex items-center gap-2 border-b border-default px-3 py-1.5">
      <UFieldGroup size="xs">
        <UButton :label="t('pages.local.files')" icon="i-lucide-files" :color="view === 'files' ? 'primary' : 'neutral'" :variant="view === 'files' ? 'solid' : 'outline'" @click="view = 'files'" />
        <UButton :label="t('pages.local.functions')" icon="i-lucide-braces" :color="view === 'functions' ? 'primary' : 'neutral'" :variant="view === 'functions' ? 'solid' : 'outline'" @click="view = 'functions'" />
      </UFieldGroup>
      <UPopover :content="{ align: 'end' }">
        <UButton :label="t('pages.local.activity')" icon="i-lucide-history" size="xs" color="neutral" variant="ghost" class="ml-auto" />
        <template #content>
          <div class="grid max-h-96 w-[32rem] gap-2 overflow-y-auto p-3 text-xs">
            <p v-if="!activity.length" class="text-muted">{{ t('pages.local.noActivity') }}</p>
            <div v-for="(entry, at) in activity" :key="at" class="grid gap-0.5">
              <div class="flex items-center gap-2">
                <UIcon :name="entry.ok ? 'i-lucide-check' : 'i-lucide-x'" class="size-3.5 shrink-0" :class="entry.ok ? 'text-success' : 'text-error'" />
                <code class="min-w-0 flex-1 truncate" :title="entry.command">{{ entry.command }}</code>
                <span class="text-muted tabular-nums">{{ entry.ms }} ms · {{ when(entry.at) }}</span>
              </div>
              <pre v-if="entry.output" class="ml-5 max-h-24 overflow-auto whitespace-pre-wrap text-muted">{{ entry.output }}</pre>
            </div>
          </div>
        </template>
      </UPopover>
    </div>
    <div v-if="view === 'files' && notRepository" class="grid flex-1 place-items-center p-8 text-center text-muted">
      <div class="grid justify-items-center gap-2">
        <UIcon name="i-lucide-git-branch" class="size-6" />
        <p>{{ t('pages.local.notRepository') }}</p>
      </div>
    </div>
    <LocalChanges v-else-if="view === 'files'" />
    <ChangesByFunction v-else />
  </div>
</template>
