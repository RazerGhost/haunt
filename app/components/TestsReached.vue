<script setup lang="ts">
import type { TestRun } from '~/bindings'
import { testsReaching } from '~~/shared/testReach'

/*
 * The tests that reach a change (shared/testReach.ts): the tests whose calls lead to a changed function, each with the
 * changed functions it reaches, and the changed functions no test reaches. In a trusted folder, Run runs just those
 * tests with the project's own runner (src-tauri/src/testrun.rs); the output opens in a dialog.
 */
const props = defineProps<{ changed: Set<string> }>()
const { index, select } = useCode()
const { repo, info } = useRepositories()
const failure = useFailure()

const reach = computed(() => {
  const at = index.value
  if (!at) return null
  return testsReaching(props.changed, id => (at.callers.get(id) ?? []).map(edge => edge.from), id => !!at.functions.get(id)?.fn.test)
})
const tests = computed(() => (reach.value?.tests ?? []).flatMap((test) => {
  const located = index.value?.functions.get(test.id)
  if (!located) return []
  const reaches = [...test.reaches].map(([id, steps]) => ({ id, steps, name: index.value?.functions.get(id)?.fn.name ?? id }))
  return [{ id: test.id, title: located.fn.name, path: located.file.path, reaches }]
}))
const unreached = computed(() => (reach.value?.unreached ?? []).flatMap((id) => {
  const located = index.value?.functions.get(id)
  return located && !located.fn.anonymous && !located.fn.test ? [{ id, name: located.fn.name, path: located.file.path }] : []
}))
const files = computed(() => new Set(tests.value.map(test => test.path)).size)
const trusted = computed(() => info.value?.trusted ?? false)

const running = ref(false)
const run = shallowRef<TestRun | null>(null)
const shown = ref(false)
async function start(): Promise<void> {
  if (!repo.value || running.value || !(await failure.trustFirst())) return
  running.value = true
  run.value = null
  try {
    run.value = await haunt.runTests(repo.value, tests.value.map(test => ({ path: test.path, title: test.title })))
    shown.value = true
    void haunt.notify(run.value.stopped ? t('TestsReached.stopped') : run.value.passed ? t('TestsReached.passed') : t('TestsReached.failed'), t('TestsReached.testsIn', { count: tests.value.length, folder: folderName(repo.value) })).catch(() => {})
  } catch (error) {
    failure.failed(t('TestsReached.didNotRun'), error, t('TestsReached.couldNotRun'))
  } finally {
    running.value = false
  }
}
const stop = () => void haunt.stopTests()
</script>

<template>
  <section class="grid gap-2">
    <div class="flex items-center gap-2">
      <h2 class="min-w-0 flex-1 font-semibold text-highlighted">{{ t('TestsReached.title') }}</h2>
      <template v-if="tests.length">
        <UButton v-if="running" :label="t('common.stop')" icon="i-lucide-square" size="xs" color="neutral" variant="outline" @click="stop" />
        <UButton
          :label="running ? t('TestsReached.running') : t('TestsReached.run', { count: tests.length })"
          icon="i-lucide-play"
          size="xs"
          :loading="running"
          :disabled="running"
          :title="trusted ? t('TestsReached.runHelp') : t('TestsReached.trustFirst')"
          @click="start"
        />
        <UButton v-if="run && !running" :icon="run.passed ? 'i-lucide-circle-check' : 'i-lucide-circle-x'" :color="run.passed ? 'success' : 'error'" size="xs" variant="ghost" :aria-label="run.passed ? t('TestsReached.passedOutput') : t('TestsReached.failedOutput')" @click="shown = true" />
      </template>
    </div>
    <p v-if="!reach" class="text-xs text-muted">{{ t('TestsReached.waiting') }}</p>
    <p v-else-if="!index?.callers.size" class="text-xs text-muted">{{ t('TestsReached.noGraph') }}</p>
    <template v-else>
      <p class="text-xs text-muted">
        <template v-if="tests.length">{{ t('TestsReached.reaching', { count: tests.length, files: t('TestsReached.files', { count: files }) }) }}</template>
        <template v-else>{{ t('TestsReached.noneReach') }}</template>
        {{ t('TestsReached.limits') }}
      </p>
      <ul v-if="tests.length" class="grid gap-0.5">
        <li v-for="test in tests.slice(0, 50)" :key="test.id">
          <button type="button" class="grid w-full min-w-0 rounded px-2 py-1 text-left hover:bg-elevated" @click="select(test.id)">
            <span class="truncate text-xs text-highlighted">{{ test.title }}</span>
            <span class="truncate font-mono text-label text-muted">{{ tailOf(test.path) }} · {{ t('TestsReached.reaches', { functions: test.reaches.map(item => `${item.name}${item.steps > 1 ? ` (${t('TestsReached.steps', { count: item.steps })})` : ''}`).join(', ') }) }}</span>
          </button>
        </li>
        <li v-if="tests.length > 50" class="px-2 text-label text-muted">{{ t('TestsReached.more', { count: tests.length - 50 }) }}</li>
      </ul>
      <div v-if="unreached.length" class="grid gap-0.5">
        <p class="flex items-center gap-1 text-xs font-medium text-warning"><UIcon name="i-lucide-triangle-alert" class="size-3.5" />{{ t('TestsReached.unreached', { count: unreached.length }) }}</p>
        <ul class="grid gap-0.5">
          <li v-for="item in unreached.slice(0, 30)" :key="item.id">
            <button type="button" class="flex w-full min-w-0 items-center gap-2 rounded px-2 py-0.5 text-left hover:bg-elevated" @click="select(item.id)">
              <span class="truncate font-mono text-xs">{{ item.name }}</span>
              <span class="truncate font-mono text-label text-muted">{{ tailOf(item.path) }}</span>
            </button>
          </li>
        </ul>
      </div>
    </template>

    <UModal v-model:open="shown" :title="run?.stopped ? t('TestsReached.stopped') : run?.passed ? t('TestsReached.passed') : t('TestsReached.failed')" :ui="{ content: 'sm:max-w-4xl' }">
      <template #body>
        <div v-if="run" class="grid gap-4">
          <p v-if="!run.runs.length" class="text-sm text-muted">{{ t('TestsReached.nothingRan') }}</p>
          <section v-for="item in run.runs" :key="item.folder + item.command" class="grid gap-1">
            <p class="flex items-center gap-2 text-xs">
              <UIcon :name="item.passed ? 'i-lucide-circle-check' : 'i-lucide-circle-x'" class="size-4 shrink-0" :class="item.passed ? 'text-success' : 'text-error'" />
              <span class="font-mono text-muted">{{ item.folder || '.' }}</span>
              <span class="min-w-0 truncate font-mono text-highlighted" :title="item.command">{{ item.command }}</span>
            </p>
            <pre class="max-h-[28rem] overflow-auto rounded-md border border-default bg-elevated/50 p-3 font-mono text-xs whitespace-pre-wrap">{{ item.output || t('TestsReached.noOutput') }}</pre>
          </section>
        </div>
      </template>
    </UModal>
  </section>
</template>
