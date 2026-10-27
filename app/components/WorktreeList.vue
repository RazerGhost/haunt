<script setup lang="ts">
/*
 * The sidebar's worktrees (useWorktrees): this repository's folders with a branch checked out each. Click one to open
 * it in its tab; right-click to open or remove it; + makes a new one. Shown once there is more than the repository's
 * own folder, and always with its + in the section's header.
 */
const { repo } = useRepositories()
const worktrees = useWorktrees()
watch([repo, worktrees.changed], () => void worktrees.load(), { immediate: true })
const props = defineProps<{ shut: boolean }>()
const emit = defineEmits<{ toggle: [] }>()
const others = computed(() => worktrees.list.value.filter(worktree => !worktree.main))
const prunable = computed(() => worktrees.list.value.some(worktree => worktree.prunable))
const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path
function menu(worktree: (typeof worktrees.list.value)[number]) {
  return [
    [{ label: t('WorktreeList.open'), icon: 'i-lucide-folder-git-2', disabled: worktree.prunable, onSelect: () => worktrees.open(worktree.path) }],
    // Not the folder on screen: switch to another tab to remove it.
    [{ label: worktree.path === repo.value ? t('WorktreeList.removeElsewhere') : t('WorktreeList.remove'), icon: 'i-lucide-trash-2', color: 'error' as const, disabled: worktree.main || worktree.path === repo.value, onSelect: () => worktrees.remove(worktree) }]
  ]
}
</script>

<template>
  <section class="grid grid-cols-[minmax(0,1fr)] gap-0.5">
    <button type="button" class="flex items-center gap-1 px-2 text-label font-semibold tracking-wider text-muted uppercase hover:text-default" :aria-expanded="!props.shut" @click="emit('toggle')">
      <UIcon name="i-lucide-chevron-right" class="size-3 transition" :class="{ 'rotate-90': !props.shut }" />
      {{ t('WorktreeList.title') }} <span class="ml-auto font-normal tabular-nums">{{ others.length }}</span>
      <UTooltip :text="t('WorktreeList.newHelp')">
        <span role="button" tabindex="0" class="-mr-1 grid size-5 place-items-center rounded normal-case hover:bg-elevated hover:text-highlighted" :aria-label="t('WorktreeList.new')" @click.stop="worktrees.add()" @keydown.enter.stop="worktrees.add()">
          <UIcon name="i-lucide-plus" class="size-3.5" />
        </span>
      </UTooltip>
    </button>
    <ul v-if="!props.shut && others.length" class="grid grid-cols-[minmax(0,1fr)]">
      <li v-for="worktree in worktrees.list.value" :key="worktree.path">
        <UContextMenu :items="menu(worktree)">
          <button
            type="button"
            class="flex w-full items-center gap-1.5 rounded py-1 pr-2 pl-5 text-left text-ui hover:bg-elevated disabled:opacity-60"
            :class="{ 'bg-elevated font-medium text-highlighted': worktree.path === repo }"
            :title="`${worktree.path}${worktree.prunable ? ` ${t('WorktreeList.folderGone')}` : ''}${worktree.locked !== null ? ` ${t('WorktreeList.locked')}` : ''}`"
            :disabled="worktree.prunable"
            @click="worktree.path !== repo && worktrees.open(worktree.path)"
          >
            <UIcon :name="worktree.main ? 'i-lucide-folder-git-2' : 'i-lucide-git-fork'" class="size-3.5 shrink-0 text-dimmed" />
            <span class="min-w-0 flex-1 truncate" :class="{ 'line-through': worktree.prunable }">{{ worktree.branch ?? t('WorktreeList.detached', { commit: worktree.head.slice(0, 7) }) }}</span>
            <span class="min-w-0 truncate text-label text-muted">{{ worktree.path === repo ? t('WorktreeList.thisFolder') : nameOf(worktree.path) }}</span>
          </button>
        </UContextMenu>
      </li>
      <li v-if="prunable" class="pl-5">
        <UButton :label="t('WorktreeList.prune')" size="xs" color="neutral" variant="ghost" @click="worktrees.prune()" />
      </li>
    </ul>
  </section>
</template>
