<script setup lang="ts">
/*
 * Asks once per folder whether the project's own code may run, in one line. Until then Haunt only reads the project's
 * files. Asked only when the folder has something to run; See what it runs shows each file first (TrustDetails), and
 * the shield in the toolbar has the details too.
 */
const { info, setTrust } = useRepositories()
const { trustDismissed: dismissed } = usePreferences()
const problem = ref('')
const busy = ref(false)
const reading = ref(false)

const shown = computed(() => !!info.value?.root && !info.value.trusted && info.value.wants.length > 0 && !dismissed.value.includes(info.value.root))

async function trust() {
  busy.value = true
  problem.value = (await setTrust(true)) ?? ''
  busy.value = false
}
function notNow() {
  if (info.value?.root) dismissed.value = [...dismissed.value, info.value.root]
}
</script>

<template>
  <div v-if="shown" class="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-default bg-elevated/60 px-4 py-2 text-sm">
    <UIcon name="i-lucide-shield-question" class="size-4 shrink-0 text-muted" />
    <p class="min-w-0 flex-1 text-muted">
      <span class="font-medium text-highlighted">Trust this folder?</span>
      Haunt would also run its
      <template v-for="(want, i) in info!.wants" :key="want.path"><template v-if="i">{{ i === info!.wants.length - 1 ? ' and ' : ', ' }}</template><span :title="want.path" class="text-default">{{ want.label }}</span></template>.
      <span v-if="problem" class="text-error">{{ problem }}</span>
    </p>
    <UButton label="See what it runs" size="xs" color="neutral" variant="outline" @click="reading = true" />
    <UButton label="Trust" icon="i-lucide-shield-check" size="xs" :loading="busy" @click="trust" />
    <UButton label="Not now" size="xs" color="neutral" variant="ghost" @click="notNow" />
    <TrustDetails v-model:open="reading" />
  </div>
</template>
