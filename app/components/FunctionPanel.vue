<script setup lang="ts">
// The selected function: where it is written, what it calls and what calls it.
const { scan, index, selected, select, open } = useCode()
const webs = useWeb()
const copyWhere = () => selected.value && navigator.clipboard?.writeText(`${selected.value.file.path}:${selected.value.fn.line}`)
const { refs } = useRefs()
const pins = usePins()

const parents = computed(() => {
  const chain: string[] = []
  let parent = selected.value?.parent ?? null
  while (parent) {
    chain.unshift(parent)
    parent = index.value?.functions.get(parent)?.parent ?? null
  }
  return chain
})
const byCount = (a: { count: number }, b: { count: number }) => b.count - a.count
const callees = computed(() => [...index.value?.callees.get(selected.value?.fn.id ?? '') ?? []].sort(byCount))
const callers = computed(() => [...index.value?.callers.get(selected.value?.fn.id ?? '') ?? []].sort(byCount))
const part = computed(() => selected.value ? index.value?.parts.get(selected.value.file.part) : undefined)
const { changedById, affected, changes } = useChanges()
const change = computed(() => changedById.value.get(selected.value?.fn.id ?? ''))
const reach = computed(() => affected.value.get(selected.value?.fn.id ?? ''))
</script>

<template>
  <div v-if="selected" class="grid gap-5 p-4" :style="{ '--c': layerColor(part?.layer ?? '') }">
    <div class="grid gap-1.5">
      <div class="flex items-start justify-between gap-2">
        <div class="font-mono text-xs tracking-wider text-(--c) uppercase">{{ t(`FunctionPanel.kind.${selected.fn.kind}`) }} · {{ selected.file.part }}</div>
        <UButton icon="i-lucide-x" size="xs" color="neutral" variant="ghost" :aria-label="t('common.close')" class="-mt-1" @click="select(null)" />
      </div>
      <h2 class="font-mono text-base font-semibold break-all text-highlighted">{{ selected.fn.name }}</h2>
      <div class="flex flex-wrap gap-1">
        <UBadge v-if="selected.fn.exported" :label="t('FunctionPanel.exported')" size="sm" variant="soft" color="info" />
        <UBadge v-if="selected.fn.async" label="async" size="sm" variant="soft" color="secondary" />
        <UBadge :label="t('FunctionPanel.lines', { count: selected.fn.endLine - selected.fn.line + 1 })" size="sm" variant="outline" color="neutral" />
        <UBadge v-if="change" :label="`${t('FunctionPanel.since', { status: t(`FunctionPanel.status.${change.status}`), base: said(changes?.base.label ?? '') })} · +${change.added} −${change.removed}`" size="sm" variant="soft" :color="change.status === 'added' ? 'success' : change.status === 'moved' ? 'info' : 'warning'" />
        <UBadge v-else-if="reach" :label="t('FunctionPanel.reach', { count: reach.distance })" size="sm" variant="outline" color="warning" />
      </div>
      <button type="button" class="text-left font-mono text-xs break-all text-muted hover:underline" @click="open(selected.file.path, selected.fn.line)">
        {{ selected.file.path }}:{{ selected.fn.line }}
      </button>
    </div>

    <div class="flex flex-wrap gap-2">
      <UButton icon="i-lucide-square-arrow-out-up-right" :label="t('FunctionPanel.openInEditor')" size="sm" @click="open(selected.file.path, selected.fn.line)" />
      <UButton icon="i-lucide-git-fork" :label="t('FunctionPanel.callGraph')" size="sm" color="neutral" variant="outline" :to="{ path: '/calls', query: { fn: selected.fn.id } }" />
      <UButton icon="i-lucide-layers" :label="t('FunctionPanel.inPart')" size="sm" color="neutral" variant="ghost" :to="{ path: '/layers', query: { part: selected.file.part, fn: selected.fn.id } }" />
      <UButton icon="i-lucide-user-round-search" :label="t('FunctionPanel.blame')" size="sm" color="neutral" variant="ghost" :to="{ path: '/blame', query: { path: selected.file.path, fn: selected.fn.id } }" />
      <UButton :icon="pins.isPinned(selected) ? 'i-lucide-pin-off' : 'i-lucide-pin'" :label="pins.isPinned(selected) ? t('FunctionPanel.unpin') : t('FunctionPanel.pin')" size="sm" color="neutral" variant="ghost" :title="pins.isPinned(selected) ? t('FunctionPanel.unpinHelp') : t('FunctionPanel.pinHelp')" @click="pins.toggle(selected)" />
      <UButton icon="i-lucide-copy" :label="t('FunctionPanel.copyWhere')" size="sm" color="neutral" variant="ghost" @click="copyWhere" />
      <UButton v-if="webs.label.value && refs?.sha" icon="i-lucide-external-link" :label="webs.label.value" size="sm" color="neutral" variant="ghost" @click="webs.openFile(refs.sha!, selected.file.path, selected.fn.line, selected.fn.endLine)" />
      <UButton v-if="webs.copyLabel.value && refs?.sha" icon="i-lucide-link" :label="webs.copyLabel.value" size="sm" color="neutral" variant="ghost" :title="t('FunctionPanel.linkHelp')" @click="webs.copyFile(refs.sha!, selected.file.path, selected.fn.line, selected.fn.endLine)" />
    </div>

    <FunctionCode :path="selected.file.path" :start="selected.fn.line" :end="selected.fn.endLine" />

    <section v-if="parents.length" class="grid gap-1.5">
      <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('FunctionPanel.writtenIn') }}</h3>
      <ul class="grid gap-1">
        <FunctionLink v-for="id in parents" :id="id" :key="id" />
      </ul>
    </section>

    <section v-if="selected.fn.children.length" class="grid gap-1.5">
      <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('FunctionPanel.writtenInside', { count: selected.fn.children.length }) }}</h3>
      <ul class="grid gap-1">
        <FunctionLink v-for="child in selected.fn.children" :id="child.id" :key="child.id" />
      </ul>
    </section>

    <p v-if="!scan?.calls" class="flex items-center gap-2 text-sm text-muted">
      <UIcon name="i-lucide-loader-circle" class="size-4 animate-spin" /> {{ t('FunctionPanel.workingOut') }}
    </p>
    <template v-else>
      <section class="grid gap-1.5">
        <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('FunctionPanel.calls', { count: callees.length }) }}</h3>
        <ul v-if="callees.length" class="grid gap-1">
          <FunctionLink v-for="edge in callees" :id="edge.to" :key="edge.to" :count="edge.count" :guessed="edge.guessed" />
        </ul>
        <p v-else class="text-xs text-muted">{{ t('FunctionPanel.noCalls') }}</p>
      </section>
      <section class="grid gap-1.5">
        <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">{{ t('FunctionPanel.calledBy', { count: callers.length }) }}</h3>
        <ul v-if="callers.length" class="grid gap-1">
          <FunctionLink v-for="edge in callers" :id="edge.from" :key="edge.from" :count="edge.count" :guessed="edge.guessed" />
        </ul>
        <p v-else class="text-xs text-muted">{{ t('FunctionPanel.noCallers') }}</p>
      </section>
    </template>

    <FunctionHistory :path="selected.file.path" :start="selected.fn.line" :end="selected.fn.endLine" :name="selected.fn.anonymous ? undefined : selected.fn.name" />
  </div>
</template>
