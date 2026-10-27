<script setup lang="ts">
import { cloneName } from '~~/shared/repositories'

/*
 * A new repository, from the repository manager: a clone of a URL, or an empty one (`git init`), in a new folder
 * under one you pick (your first repository folder to start with). A clone shows git's progress as it comes and can
 * be stopped; git's questions (a password, a host key) come up as they do for a fetch. Done, the new folder opens.
 */
const props = defineProps<{ mode: 'clone' | 'new', folders: string[] }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ created: [path: string] }>()
const { info, onNews } = useRepositories()

const inApp = import.meta.client && '__TAURI__' in window
const url = ref('')
const parent = ref('')
const name = ref('')
/** The name follows the URL until it is typed in. */
const named = ref(false)
const busy = ref(false)
const problem = ref('')
const progress = ref('')

watch(open, (now) => {
  if (!now) return
  url.value = ''
  name.value = ''
  named.value = false
  problem.value = ''
  progress.value = ''
  const root = info.value?.root
  parent.value = props.folders[0] ?? (root ? root.replace(/[\\/][^\\/]+[\\/]?$/, '') : '')
})
watch(url, (now) => {
  if (!named.value) name.value = cloneName(now)
})

async function browse() {
  const { open: pick } = await import('@tauri-apps/plugin-dialog')
  const chosen = await pick({ directory: true, title: t('RepoCreate.parentTitle'), defaultPath: parent.value || undefined })
  if (typeof chosen === 'string') parent.value = chosen
}

const stopListening = onNews((news) => {
  if (news.type === 'clone' && busy.value) progress.value = news.done ? '' : news.text ?? progress.value
})
onBeforeUnmount(stopListening)

const ready = computed(() => !!parent.value.trim() && !!name.value.trim() && (props.mode === 'new' || !!url.value.trim()))
const separator = computed(() => (parent.value.includes('\\') ? '\\' : '/'))
const target = computed(() => (parent.value.trim() && name.value.trim() ? `${parent.value.trim().replace(/[\\/]+$/, '')}${separator.value}${name.value.trim()}` : ''))

async function create() {
  if (!ready.value || busy.value) return
  busy.value = true
  problem.value = ''
  try {
    const made = props.mode === 'clone'
      ? await haunt.cloneRepository(url.value.trim(), parent.value.trim(), name.value.trim())
      : await haunt.initRepository(parent.value.trim(), name.value.trim())
    open.value = false
    emit('created', made)
  } catch (error) {
    problem.value = errorMessage(error, props.mode === 'clone' ? t('RepoCreate.notCloned') : t('RepoCreate.notMade'))
  } finally {
    busy.value = false
  }
}
async function stop() {
  await haunt.stopClone(parent.value.trim()).catch(() => false)
}
</script>

<template>
  <UModal v-model:open="open" :title="mode === 'clone' ? t('RepoCreate.cloneTitle') : t('RepoCreate.newTitle')" :dismissible="!busy" :close="!busy">
    <template #body>
      <form class="grid gap-4" @submit.prevent="create">
        <UFormField v-if="mode === 'clone'" :label="t('RepoCreate.url')" :help="t('RepoCreate.urlHelp')">
          <UInput v-model="url" class="w-full" :ui="{ base: 'font-mono' }" placeholder="https://github.com/owner/repo.git" autofocus :disabled="busy" />
        </UFormField>
        <UFormField :label="t('RepoCreate.parent')">
          <div class="flex gap-2">
            <UInput v-model="parent" class="flex-1" :ui="{ base: 'font-mono' }" :disabled="busy" />
            <UButton v-if="inApp" icon="i-lucide-folder-open" color="neutral" variant="outline" :aria-label="t('RepoCreate.parentTitle')" :disabled="busy" @click="browse" />
          </div>
        </UFormField>
        <UFormField :label="t('RepoCreate.name')" :help="target ? t('RepoCreate.goesTo', { path: target }) : undefined">
          <UInput v-model="name" class="w-full" :ui="{ base: 'font-mono' }" :autofocus="mode === 'new'" :disabled="busy" @update:model-value="named = true" />
        </UFormField>
        <p v-if="mode === 'clone'" class="text-xs text-muted">{{ t('RepoCreate.cloneNote') }}</p>
        <p v-if="busy && mode === 'clone'" class="truncate font-mono text-xs text-muted">{{ progress || t('RepoCreate.starting') }}</p>
        <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" class="whitespace-pre-wrap" />
        <div class="flex justify-end gap-2">
          <UButton v-if="busy && mode === 'clone'" :label="t('common.stop')" color="neutral" variant="outline" @click="stop" />
          <UButton v-else :label="t('common.cancel')" color="neutral" variant="ghost" @click="open = false" />
          <UButton type="submit" :label="mode === 'clone' ? t('RepoCreate.clone') : t('RepoCreate.create')" :icon="mode === 'clone' ? 'i-lucide-download' : 'i-lucide-folder-plus'" :loading="busy" :disabled="!ready" />
        </div>
      </form>
    </template>
  </UModal>
</template>
