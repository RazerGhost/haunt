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
  if (!repo.value || running.value) return
  running.value = true
  run.value = null
  try {
    run.value = await haunt.runTests(repo.value, tests.value.map(test => ({ path: test.path, title: test.title })))
    shown.value = true
    void haunt.notify(run.value.stopped ? 'Tests stopped' : run.value.passed ? 'Tests passed' : 'Tests failed', `${tests.value.length} ${tests.value.length === 1 ? 'test' : 'tests'} in ${folderName(repo.value)}`).catch(() => {})
  } catch (error) {
    failure.failed('The tests did not run', error, 'The tests could not run.')
  } finally {
    running.value = false
  }
}
const stop = () => void haunt.stopTests()
</script>

<template>
  <section class="grid gap-2">
    <div class="flex items-center gap-2">
      <h2 class="min-w-0 flex-1 font-semibold text-highlighted">Tests that reach it</h2>
      <template v-if="tests.length">
        <UButton v-if="running" label="Stop" icon="i-lucide-square" size="xs" color="neutral" variant="outline" @click="stop" />
        <UButton
          :label="running ? 'Running…' : `Run ${tests.length === 1 ? 'it' : `these ${tests.length}`}`"
          icon="i-lucide-play"
          size="xs"
          :loading="running"
          :disabled="!trusted || running"
          :title="trusted ? 'Runs these tests with the project\'s test runner' : 'Tests run the project\'s code: trust this folder first'"
          @click="start"
        />
        <UButton v-if="run && !running" :icon="run.passed ? 'i-lucide-circle-check' : 'i-lucide-circle-x'" :color="run.passed ? 'success' : 'error'" size="xs" variant="ghost" :aria-label="run.passed ? 'Passed: see the output' : 'Failed: see the output'" @click="shown = true" />
      </template>
    </div>
    <p v-if="!reach" class="text-xs text-muted">Waiting for the scan.</p>
    <p v-else-if="!index?.callers.size" class="text-xs text-muted">The call graph is still being worked out.</p>
    <template v-else>
      <p class="text-xs text-muted">
        <template v-if="tests.length">{{ tests.length }} {{ tests.length === 1 ? 'test' : 'tests' }} in {{ files }} {{ files === 1 ? 'file' : 'files' }} reach a changed function.</template>
        <template v-else>No test reaches a changed function.</template>
        TypeScript and JavaScript tests only; calls through interfaces or callbacks are not followed.
      </p>
      <ul v-if="tests.length" class="grid gap-0.5">
        <li v-for="test in tests.slice(0, 50)" :key="test.id">
          <button type="button" class="grid w-full min-w-0 rounded px-2 py-1 text-left hover:bg-elevated" @click="select(test.id)">
            <span class="truncate text-xs text-highlighted">{{ test.title }}</span>
            <span class="truncate font-mono text-label text-muted">{{ tailOf(test.path) }} · reaches {{ test.reaches.map(item => `${item.name}${item.steps > 1 ? ` (${item.steps} steps)` : ''}`).join(', ') }}</span>
          </button>
        </li>
        <li v-if="tests.length > 50" class="px-2 text-label text-muted">And {{ tests.length - 50 }} more.</li>
      </ul>
      <div v-if="unreached.length" class="grid gap-0.5">
        <p class="flex items-center gap-1 text-xs font-medium text-warning"><UIcon name="i-lucide-triangle-alert" class="size-3.5" />No test reaches {{ unreached.length === 1 ? 'this changed function' : `these ${unreached.length} changed functions` }}</p>
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

    <UModal v-model:open="shown" :title="run?.stopped ? 'Tests stopped' : run?.passed ? 'Tests passed' : 'Tests failed'" :ui="{ content: 'sm:max-w-4xl' }">
      <template #body>
        <div v-if="run" class="grid gap-4">
          <p v-if="!run.runs.length" class="text-sm text-muted">Nothing ran.</p>
          <section v-for="item in run.runs" :key="item.folder + item.command" class="grid gap-1">
            <p class="flex items-center gap-2 text-xs">
              <UIcon :name="item.passed ? 'i-lucide-circle-check' : 'i-lucide-circle-x'" class="size-4 shrink-0" :class="item.passed ? 'text-success' : 'text-error'" />
              <span class="font-mono text-muted">{{ item.folder || '.' }}</span>
              <span class="min-w-0 truncate font-mono text-highlighted" :title="item.command">{{ item.command }}</span>
            </p>
            <pre class="max-h-[28rem] overflow-auto rounded-md border border-default bg-elevated/50 p-3 font-mono text-xs whitespace-pre-wrap">{{ item.output || '(no output)' }}</pre>
          </section>
        </div>
      </template>
    </UModal>
  </section>
</template>
