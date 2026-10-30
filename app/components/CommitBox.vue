<script setup lang="ts">
/*
 * The commit box: a subject and a body, amend (the last commit's message comes in to edit) and sign-off. In a folder
 * you have not trusted, its commit hooks and signing do not run, and the box says so before you commit.
 *
 * What is written is kept as a draft per repository and branch, so switching tabs or branches loses nothing; it goes
 * once the commit is made. The subject counts up to 50 characters and the body's lines up to 72 (git's own advice),
 * and Recent offers the subjects of the branch's last commits to start from. Ctrl+Enter commits. With `amend` it starts
 * in amend mode (Local changes' Amend, with nothing changed), and says `done` once the commit is made or amend is unticked.
 *
 * Draft writes a message from what is staged, by function (shared/commitDraft.ts): what it adds, changes and removes,
 * and the callers outside the change. Worked out here, without AI: nothing leaves the machine. It says what, not why.
 */
import { draftCommitMessage } from '~~/shared/commitDraft'

const props = defineProps<{ staged: number, amend?: boolean }>()
const emit = defineEmits<{ done: [] }>()
const { commitBox, commit, busy } = useLocalChanges()
const { repo } = useRepositories()
const { current } = useRefs()
const { index } = useCode()
const failure = useFailure()
const dialog = useDialog()

const subject = ref('')
const body = ref('')
const amend = ref(false)
// Sign-off starts as Settings says (Git: sign off by default).
const signoff = ref(usePreferences().signoff.value)

/* Drafts, per repository and branch */
type Draft = { subject: string, body: string }
const drafts = usePersisted<Record<string, Draft>>('commit-drafts', {}, value => !!value && typeof value === 'object' && !Array.isArray(value))
const draftKey = computed(() => (repo.value ? `${repo.value}\n${current.value?.name ?? 'HEAD'}` : null))
let restoring = false
watch(draftKey, (key) => {
  restoring = true
  const draft = key ? drafts.value[key] : undefined
  subject.value = draft?.subject ?? ''
  body.value = draft?.body ?? ''
  amend.value = false
  void nextTick(() => { restoring = false })
}, { immediate: true })
watch([subject, body], ([nextSubject, nextBody]) => {
  const key = draftKey.value
  if (restoring || !key || amend.value) return
  const { [key]: _old, ...rest } = drafts.value
  drafts.value = nextSubject.trim() || nextBody.trim() ? { ...rest, [key]: { subject: nextSubject, body: nextBody } } : rest
})

// Amending a commit a remote has rewrites it there too: said before committing, as undo and reset say it.
const amendPushed = ref<string[]>([])
watch(amend, async (now) => {
  amendPushed.value = now && repo.value ? await haunt.onRemotes(repo.value, 'HEAD').catch(() => []) : []
})

// Amending starts from the last commit's message, unless something is written already.
watch(amend, (now) => {
  if (!now || subject.value || body.value) return
  const [first = '', ...rest] = (commitBox.value?.lastMessage ?? '').split('\n')
  subject.value = first
  body.value = rest.join('\n').replace(/^\n+/, '')
})
if (props.amend) amend.value = true
// Opened to amend: unticking it closes the box again.
watch(amend, (now) => { if (props.amend && !now) emit('done') })

/* Lengths: git's advice is a subject of at most 50 characters, and body lines of at most 72 */
const subjectLength = computed(() => subject.value.trim().length)
const longLines = computed(() => body.value.split('\n').filter(line => line.length > 72).length)

/* Recent subjects of the branch, to start from */
const recent = shallowRef<string[]>([])
async function loadRecent(): Promise<void> {
  if (!repo.value) return
  try {
    const page = await haunt.log(repo.value, { from: ['HEAD'], limit: 30 })
    recent.value = [...new Set(page.commits.map(item => item.subject).filter(text => !/^Merge /.test(text)))].slice(0, 12)
  } catch { recent.value = [] }
}
const recentItems = computed(() => [recent.value.map(text => ({ label: text, onSelect: () => { subject.value = text } }))])

/* A draft from the staged changes */
const drafting = ref(false)
async function draft(): Promise<void> {
  const at = repo.value
  if (!at || drafting.value) return
  if ((subject.value.trim() || body.value.trim()) && !await dialog.confirm({ title: t('CommitBox.replaceTitle'), description: t('CommitBox.replaceDescription'), confirm: t('CommitBox.replace') })) return
  drafting.value = true
  try {
    const staged = await haunt.stagedChanges(at)
    const callers = new Map([...countsByPart(index.value, staged.files, namedOnly(index.value, blastRadius(index.value, changedIds(staged.files), 1)))].map(([part, counts]) => [part, counts.affected]))
    const made = draftCommitMessage(staged.files, callers)
    subject.value = made.subject
    body.value = made.body
  } catch (error) {
    failure.failed(t('CommitBox.noDraft'), error, t('CommitBox.stagedNotRead'))
  } finally {
    drafting.value = false
  }
}

const notes = computed(() => {
  const box = commitBox.value
  if (!box || box.trusted) return []
  const out: string[] = []
  if (box.hooks.length) out.push(t('CommitBox.hooksSkipped', { count: box.hooks.length, hooks: box.hooks.join(', ') }))
  if (box.signing) out.push(t('CommitBox.notSigned'))
  return out
})
const ready = computed(() => !!subject.value.trim() && (props.staged > 0 || amend.value) && !busy.value)

async function submit(): Promise<void> {
  if (!ready.value) return
  const message = body.value.trim() ? `${subject.value.trim()}\n\n${body.value.trim()}` : subject.value.trim()
  const key = draftKey.value
  if (await commit(message, amend.value, signoff.value)) {
    subject.value = ''
    body.value = ''
    amend.value = false
    if (key) {
      const { [key]: _done, ...rest } = drafts.value
      drafts.value = rest
    }
    emit('done')
  }
}
</script>

<template>
  <form class="grid gap-2 p-3" @submit.prevent="submit">
    <div class="flex items-center gap-1.5">
      <UInput v-model="subject" :placeholder="t('CommitBox.subject')" size="sm" class="min-w-0 flex-1" :aria-label="t('CommitBox.subjectLabel')" @keydown.ctrl.enter="submit" @keydown.meta.enter="submit">
        <template v-if="subjectLength" #trailing>
          <span class="text-label tabular-nums" :class="subjectLength > 72 ? 'text-error' : subjectLength > 50 ? 'text-warning' : 'text-dimmed'" :title="subjectLength > 50 ? t('CommitBox.subjectLong') : undefined">{{ subjectLength }}</span>
        </template>
      </UInput>
      <UDropdownMenu :items="recentItems" :content="{ align: 'end' }" @update:open="(open: boolean) => { if (open) void loadRecent() }">
        <UButton icon="i-lucide-history" size="sm" color="neutral" variant="ghost" :aria-label="t('CommitBox.recent')" :title="t('CommitBox.recentHelp')" />
      </UDropdownMenu>
      <UButton icon="i-lucide-file-pen-line" size="sm" color="neutral" variant="ghost" :aria-label="t('CommitBox.draft')" :title="t('CommitBox.draftHelp')" :disabled="!props.staged" :loading="drafting" @click="draft" />
    </div>
    <UTextarea v-model="body" :placeholder="t('CommitBox.description')" size="sm" :rows="3" autoresize :maxrows="10" :aria-label="t('CommitBox.descriptionLabel')" @keydown.ctrl.enter="submit" @keydown.meta.enter="submit" />
    <p v-if="longLines" class="text-xs text-warning">{{ t('CommitBox.longLines', { count: longLines }) }}</p>
    <p v-if="amend && amendPushed.length" class="flex items-start gap-1.5 text-xs text-warning">
      <UIcon name="i-lucide-triangle-alert" class="mt-0.5 size-3.5 shrink-0" />{{ t('CommitBox.amendPushed', { remotes: amendPushed.join(', ') }) }}
    </p>
    <p v-for="note in notes" :key="note" class="flex items-start gap-1.5 text-xs text-warning">
      <UIcon name="i-lucide-shield-alert" class="mt-0.5 size-3.5 shrink-0" />{{ note }}
    </p>
    <UButton v-if="notes.length" :label="t('CommitBox.trust')" icon="i-lucide-shield-check" size="xs" color="neutral" variant="link" class="justify-self-start px-0" @click="failure.trustFirst()" />
    <div class="flex flex-wrap items-center gap-4">
      <UCheckbox v-model="amend" :label="t('CommitBox.amend')" size="sm" :disabled="!commitBox?.lastMessage" />
      <UCheckbox v-model="signoff" :label="t('CommitBox.signOff')" size="sm" />
      <span class="ml-auto text-label text-muted">Ctrl Enter</span>
      <UButton type="submit" :label="amend ? t('CommitBox.amendLast') : props.staged ? t('CommitBox.commitFiles', { count: props.staged }) : t('CommitBox.commit')" icon="i-lucide-git-commit-horizontal" size="sm" :disabled="!ready" :loading="busy" />
    </div>
  </form>
</template>
