<script setup lang="ts">
import type { Submodule } from '~/bindings'

/*
 * The sidebar's submodules (src-tauri/src/git/submodules.rs): other repositories this one has at a fixed commit. Each
 * shows whether it is checked out at that commit, at another, or not at all; click one to open it in its own tab, and
 * Update checks them out at the recorded commits, cloning the missing ones (only in a trusted folder: it clones from
 * the addresses in .gitmodules). Shown only when there are any.
 */
const { repo, info, gitChanged, chooseRoot } = useRepositories()
const network = useNetwork()
const failure = useFailure()
const props = defineProps<{ shut: boolean }>()
const emit = defineEmits<{ toggle: [] }>()

const list = shallowRef<Submodule[]>([])
const changed = gitChanged('head', 'index', 'refs')
const ask = latest(repo)
async function load(): Promise<void> {
  if (!repo.value) return
  try {
    const found = await ask.run(at => haunt.submodules(at))
    if (found !== undefined) list.value = found
  } catch { /* the list stays as it was */ }
}
// Emptied for another tab only: on a commit the list stays while it is read again, so the section does not flicker.
watch(repo, () => { list.value = [] })
watch([repo, changed], () => void load(), { immediate: true })

const trusted = computed(() => info.value?.trusted ?? false)
const behind = computed(() => list.value.filter(item => item.state !== 'current').length)
const STATE = {
  current: { icon: 'i-lucide-package-check' },
  moved: { icon: 'i-lucide-package-open' },
  missing: { icon: 'i-lucide-package-x' }
} as const

const folderOf = (item: Submodule) => `${repo.value}${repo.value?.includes('\\') ? '\\' : '/'}${item.path.replaceAll('/', repo.value?.includes('\\') ? '\\' : '/')}`
async function update(paths: string[]): Promise<void> {
  const at = repo.value
  if (!at || network.running.value) return
  try {
    await haunt.updateSubmodules(at, paths)
    await load()
  } catch (error) {
    failure.failed(t('SubmoduleList.notUpdated'), error)
  }
}
function menu(item: Submodule) {
  return [[
    { label: t('SubmoduleList.open'), icon: 'i-lucide-folder-git-2', disabled: item.state === 'missing', onSelect: () => chooseRoot(folderOf(item)) },
    { label: item.state === 'missing' ? t('SubmoduleList.clone') : t('SubmoduleList.checkOut'), icon: 'i-lucide-refresh-cw', disabled: !trusted.value || item.state === 'current', onSelect: () => update([item.path]) }
  ], [
    { label: t('SubmoduleList.copyAddress'), icon: 'i-lucide-copy', disabled: !item.url, onSelect: () => navigator.clipboard?.writeText(item.url) }
  ]]
}
</script>

<template>
  <section v-if="list.length" class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
    <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!props.shut" @click="emit('toggle')">
      <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !props.shut }" />
      {{ t('SubmoduleList.title') }} <span class="ml-auto font-normal tabular-nums">{{ list.length }}</span>
      <UTooltip v-if="behind" :text="trusted ? t('SubmoduleList.updateHelp') : t('SubmoduleList.trustFirst')">
        <span role="button" tabindex="0" class="-mr-1 grid size-5 place-items-center rounded normal-case hover:bg-elevated hover:text-highlighted" :class="{ 'opacity-50': !trusted }" :aria-label="t('SubmoduleList.update')" @click.stop="trusted && update([])" @keydown.enter.stop="trusted && update([])">
          <UIcon name="i-lucide-refresh-cw" class="size-3.5" />
        </span>
      </UTooltip>
    </button>
    <ul v-if="!props.shut" class="grid grid-cols-[minmax(0,1fr)]">
      <li v-for="item in list" :key="item.path">
        <UContextMenu :items="menu(item)">
          <button
            type="button"
            class="flex w-full items-center gap-1.5 rounded py-1 pr-2 pl-5 text-left text-ui hover:bg-elevated"
            :title="`${item.path}: ${t(`SubmoduleList.state.${item.state}`)}${item.url ? `\n${t('SubmoduleList.from', { url: item.url })}` : ''}`"
            @click="item.state === 'missing' ? (trusted && update([item.path])) : chooseRoot(folderOf(item))"
          >
            <UIcon :name="STATE[item.state].icon" class="size-3.5 shrink-0" :class="item.state === 'current' ? 'text-dimmed' : 'text-warning'" />
            <span class="min-w-0 flex-1 truncate">{{ item.path }}</span>
            <span v-if="item.state !== 'current'" class="text-label text-warning">{{ item.state === 'missing' ? t('SubmoduleList.notThere') : t('SubmoduleList.moved') }}</span>
          </button>
        </UContextMenu>
      </li>
    </ul>
  </section>
</template>
