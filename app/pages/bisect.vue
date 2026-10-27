<script setup lang="ts">
/*
 * Finding the commit that broke something (useBisect): start from a bad commit (where you are, by default) and one that
 * was still good; Haunt checks out commits in between, and you say whether each is good or bad (or skip it) until only
 * the first bad one is left. Started with a function picked, the commits that touched it come first: any of them can be
 * tested instead of git's pick.
 */
const route = useRoute()
const { selected } = useCode()
const { current } = useRefs()
const { times } = usePreferences()
const { state, busy, start, mark, jump, stop, suggested } = useBisect()

const bad = ref('HEAD')
const good = ref(typeof route.query.good === 'string' ? route.query.good : '')
watch(() => route.query.good, (now) => { if (typeof now === 'string') good.value = now })
const useFunction = ref(true)
const fromFunction = computed(() => (selected.value && !selected.value.fn.anonymous
  ? { path: selected.value.file.path, start: selected.value.fn.line, end: selected.value.fn.endLine, name: selected.value.fn.name }
  : null))
async function begin(): Promise<void> {
  if (!good.value.trim()) return
  await start(bad.value.trim() || 'HEAD', good.value.trim(), useFunction.value ? fromFunction.value ?? undefined : undefined)
}
const when = (seconds: number) => times.value === 'date' ? new Date(seconds * 1000).toLocaleString(locale()) : new Date(seconds * 1000).toLocaleDateString(locale())
</script>

<template>
  <div class="mx-auto grid max-w-2xl content-start gap-5 px-4 py-8">
    <div class="grid gap-1">
      <h1 class="text-lg font-semibold text-highlighted">{{ t('pages.bisect.title') }}</h1>
      <p class="text-sm text-muted">{{ t('pages.bisect.intro') }}</p>
    </div>

    <form v-if="!state?.active" class="grid gap-4" @submit.prevent="begin">
      <UFormField :label="t('pages.bisect.bad')" :help="current ? t('pages.bisect.badHelpOn', { branch: current.name }) : t('pages.bisect.badHelp')">
        <UInput v-model="bad" class="w-full" :ui="{ base: 'font-mono' }" />
      </UFormField>
      <UFormField :label="t('pages.bisect.good')" :help="t('pages.bisect.goodHelp')">
        <UInput v-model="good" placeholder="v1.2, a1b2c3d" class="w-full" :ui="{ base: 'font-mono' }" />
      </UFormField>
      <USwitch
        v-if="fromFunction"
        v-model="useFunction"
        :label="t('pages.bisect.functionFirst', { name: fromFunction.name })"
        :description="t('pages.bisect.functionFirstHelp')"
      />
      <p v-else class="text-xs text-muted">{{ t('pages.bisect.pickFunction') }}</p>
      <UButton type="submit" :label="t('pages.bisect.start')" icon="i-lucide-crosshair" class="justify-self-start" :disabled="!good.trim()" :loading="busy" />
    </form>

    <template v-else>
      <section v-if="state.culprit" class="grid gap-2 rounded-md border border-error/40 bg-error/5 p-4">
        <p class="text-xs font-semibold tracking-wider text-error uppercase">{{ t('pages.bisect.firstBad') }}</p>
        <p class="text-base font-medium text-highlighted">{{ state.culprit.subject }}</p>
        <p class="font-mono text-xs text-muted">{{ state.culprit.sha.slice(0, 7) }} · {{ state.culprit.author }} · {{ when(state.culprit.time) }}</p>
        <div class="flex gap-2">
          <UButton :label="t('pages.bisect.seeIt')" icon="i-lucide-git-commit-vertical" size="sm" :to="{ path: '/commits', query: { commit: state.culprit.sha } }" />
          <UButton :label="t('pages.bisect.stopBisecting')" size="sm" color="neutral" variant="outline" :loading="busy" :title="t('pages.bisect.stopHelp')" @click="stop" />
        </div>
      </section>
      <section v-else class="grid gap-3 rounded-md border border-default p-4">
        <p class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.bisect.testingNow') }}</p>
        <p class="text-base font-medium text-highlighted">{{ state.current?.subject }}</p>
        <p class="font-mono text-xs text-muted">{{ state.current?.sha.slice(0, 7) }} · {{ state.current?.author }} · {{ t('pages.bisect.inPlay', { count: state.left }) }}, {{ t('pages.bisect.stepsLeft', { count: state.steps }) }}</p>
        <p class="text-xs text-muted">{{ t('pages.bisect.tryIt') }}</p>
        <div class="flex flex-wrap gap-2">
          <UButton :label="t('pages.bisect.markGood')" icon="i-lucide-check" color="success" variant="soft" :loading="busy" @click="mark('good')" />
          <UButton :label="t('pages.bisect.markBad')" icon="i-lucide-x" color="error" variant="soft" :disabled="busy" @click="mark('bad')" />
          <UButton :label="t('pages.bisect.markSkip')" color="neutral" variant="outline" :disabled="busy" @click="mark('skip')" />
          <UButton :label="t('common.stop')" color="neutral" variant="ghost" class="ml-auto" :disabled="busy" @click="stop" />
        </div>
      </section>
      <section v-if="suggested && !state.culprit" class="grid gap-1">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('pages.bisect.touched', { name: suggested.name, count: suggested.commits.length }) }}</h2>
        <p v-if="!suggested.commits.length" class="text-xs text-muted">{{ t('pages.bisect.noneTouched') }}</p>
        <ul class="grid gap-0.5">
          <li v-for="commit in suggested.commits" :key="commit.sha" class="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-elevated">
            <span class="min-w-0 flex-1 truncate">{{ commit.subject }}</span>
            <span class="font-mono text-muted">{{ commit.sha.slice(0, 7) }}</span>
            <UBadge v-if="state.current?.sha === commit.sha" :label="t('pages.bisect.testing')" size="sm" color="info" variant="subtle" />
            <UButton v-else :label="t('pages.bisect.testThis')" size="xs" color="neutral" variant="ghost" :disabled="busy" @click="jump(commit.sha)" />
          </li>
        </ul>
      </section>
      <p class="text-xs text-muted">{{ t('pages.bisect.summary', { good: state.goods.length, skipped: state.skipped.length }) }}</p>
    </template>
  </div>
</template>
