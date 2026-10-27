<script setup lang="ts">
import type { PullDraft } from '~/bindings'

/*
 * A new pull request (a merge request on GitLab) for a branch, made through the host's own tool (gh, glab; tea for
 * Gitea) with the account you signed in with: the branch it goes into, a title and a description to start from (its
 * commits; src-tauri/src/git/remote.rs), and whether it is a draft. A branch not on the remote yet, or with commits
 * the remote lacks, is pushed first, from here.
 */
const open = defineModel<boolean>('open', { required: true })
const { repo } = useRepositories()
const { refs, current } = useRefs()
const network = useNetwork()
const toast = useToast()

const branch = ref('')
const base = ref('')
const title = ref('')
const body = ref('')
const asDraft = ref(false)
const draft = shallowRef<PullDraft | null>(null)
const problem = ref('')
const reading = ref(false)
const making = ref(false)
const branches = computed(() => (refs.value?.branches ?? []).map(item => item.name))

async function read(wantedBase: string | null): Promise<void> {
  if (!repo.value || !branch.value) return
  reading.value = true
  problem.value = ''
  try {
    const found = await haunt.pullRequestDraft(repo.value, branch.value, wantedBase)
    const edited = draft.value && (title.value !== draft.value.title || body.value !== draft.value.body)
    draft.value = found
    base.value = found.base
    // A title or description you changed stays; one Haunt made follows the branch and base.
    if (!edited) {
      title.value = found.title
      body.value = found.body
    }
  } catch (error) {
    draft.value = null
    problem.value = errorMessage(error, t('PullCreate.notRead'))
  } finally {
    reading.value = false
  }
}
watch(open, (now) => {
  if (!now) return
  draft.value = null
  branch.value = current.value?.name ?? branches.value[0] ?? ''
  title.value = ''
  body.value = ''
  asDraft.value = false
  void read(null)
})
watch(branch, (now, before) => { if (open.value && before && now !== before) void read(null) })
function pickBase(name: string): void {
  if (name !== draft.value?.base) void read(name)
}
// A push finished: whether the branch is on the remote now.
watch(() => network.running.value, (now, before) => { if (open.value && before && !now) void read(base.value || null) })

const ready = computed(() => !!draft.value && draft.value.pushed && !!title.value.trim() && !making.value)
async function create(): Promise<void> {
  if (!ready.value || !repo.value) return
  making.value = true
  problem.value = ''
  const at = repo.value
  try {
    const made = await haunt.createPullRequest(at, { branch: branch.value, base: base.value, title: title.value.trim(), body: body.value, draft: asDraft.value })
    open.value = false
    toast.add({
      title: t('PullCreate.made'),
      description: made.url ?? undefined,
      icon: 'i-lucide-git-pull-request',
      color: 'success',
      actions: made.url ? [{ label: t('PullCreate.openIt'), color: 'neutral', variant: 'outline', onClick: () => { void haunt.openOnWeb(at, made.url!).catch(() => {}) } }] : undefined
    })
  } catch (error) {
    problem.value = errorMessage(error, t('PullCreate.notMade'))
  } finally {
    making.value = false
  }
}
</script>

<template>
  <UModal v-model:open="open" :title="t('PullCreate.title')" :ui="{ content: 'max-w-2xl' }">
    <template #body>
      <form class="grid gap-4" @submit.prevent="create">
        <div class="grid grid-cols-2 gap-3">
          <UFormField :label="t('PullCreate.from')">
            <USelectMenu v-model="branch" :items="branches" icon="i-lucide-git-branch" class="w-full" />
          </UFormField>
          <UFormField :label="t('PullCreate.into', { remote: draft?.remote ?? 'origin' })">
            <USelectMenu :model-value="base" :items="draft?.bases ?? []" icon="i-lucide-git-merge" class="w-full" :disabled="!draft" @update:model-value="pickBase" />
          </UFormField>
        </div>
        <UAlert v-if="draft && !draft.pushed" color="warning" variant="subtle" icon="i-lucide-cloud-upload" :description="t('PullCreate.pushFirst', { branch, remote: draft.remote })">
          <template #actions>
            <UButton :label="t('PullCreate.push')" size="xs" color="neutral" variant="outline" :loading="!!network.running.value" :disabled="!network.trusted.value || branch !== current?.name" @click="network.push(draft.remote)" />
          </template>
        </UAlert>
        <p v-if="draft && !draft.pushed && branch !== current?.name" class="text-xs text-muted">{{ t('PullCreate.pushOther') }}</p>
        <p v-if="draft && !draft.commits.length" class="text-xs text-warning">{{ t('PullCreate.noCommits', { base: draft.base }) }}</p>
        <UFormField :label="t('PullCreate.titleLabel')">
          <UInput v-model="title" class="w-full" :disabled="reading" />
        </UFormField>
        <UFormField :label="t('PullCreate.body')" :help="draft?.commits.length ? t('PullCreate.commits', { count: draft.commits.length }) : undefined">
          <UTextarea v-model="body" :rows="8" autoresize :maxrows="16" class="w-full" :ui="{ base: 'font-mono text-xs' }" :disabled="reading" />
        </UFormField>
        <UCheckbox v-model="asDraft" :label="t('PullCreate.asDraft')" />
        <UAlert v-if="problem" color="error" variant="subtle" icon="i-lucide-circle-alert" :description="problem" class="whitespace-pre-wrap" />
        <div class="flex items-center justify-end gap-2">
          <span class="mr-auto text-xs text-muted">{{ t('PullCreate.through') }}</span>
          <UButton :label="t('common.cancel')" color="neutral" variant="ghost" @click="open = false" />
          <UButton type="submit" icon="i-lucide-git-pull-request" :label="t('PullCreate.make')" :loading="making" :disabled="!ready" />
        </div>
      </form>
    </template>
  </UModal>
</template>
