<script setup lang="ts">
// The app's one dialog (useDialog): the question asked now, if any.
const { asking } = useDialog()

const input = ref('')
const field = useTemplateRef<{ inputRef?: HTMLInputElement | null, textareaRef?: HTMLTextAreaElement | null }>('field')
const note = ref('')
const checked = ref(false)
watch(asking, (now) => {
  input.value = now?.input?.value ?? ''
  note.value = now?.note?.value ?? ''
  checked.value = now?.checkbox?.value ?? false
  // Focused at once (and again once the opening animation is done), so the first letters typed are not lost.
  if (now?.input) {
    const focus = () => (field.value?.inputRef ?? field.value?.textareaRef)?.focus()
    void nextTick(focus)
    setTimeout(focus, 60)
    setTimeout(focus, 200)
  }
})

const open = computed({ get: () => !!asking.value, set: (value) => { if (!value) asking.value?.done(null) } })
const blocked = computed(() => !!asking.value?.input && asking.value.input.required !== false && !input.value.trim())
function answer(choice: string): void {
  if (choice === 'ok' && blocked.value) return
  asking.value?.done({ choice, input: input.value, note: note.value, checked: checked.value })
}
</script>

<template>
  <UModal v-model:open="open" :title="asking?.title ?? ''" :description="asking?.description">
    <template v-if="asking?.input || asking?.checkbox" #body>
      <form class="grid gap-3" @submit.prevent="answer(asking!.choices[0]!.value)">
        <UFormField v-if="asking?.input" :label="asking.input.label">
          <UTextarea v-if="asking.input.multiline" ref="field" v-model="input" :placeholder="asking.input.placeholder" :rows="3" autoresize class="w-full" autofocus />
          <UInput v-else ref="field" v-model="input" :type="asking.input.secret ? 'password' : 'text'" :placeholder="asking.input.placeholder" class="w-full" :ui="{ base: 'font-mono' }" autocomplete="off" autofocus />
        </UFormField>
        <UFormField v-if="asking?.note" :label="asking.note.label">
          <UTextarea v-model="note" :placeholder="asking.note.placeholder" :rows="2" autoresize class="w-full" />
        </UFormField>
        <UCheckbox v-if="asking?.checkbox" v-model="checked" :label="asking.checkbox.label" />
      </form>
    </template>
    <template #footer>
      <div class="flex w-full flex-wrap justify-end gap-2">
        <UButton :label="t('common.cancel')" color="neutral" variant="ghost" @click="asking?.done(null)" />
        <UButton
          v-for="(choice, at) in asking?.choices ?? []"
          :key="choice.value"
          :label="choice.label"
          :color="choice.color ?? 'neutral'"
          :variant="choice.color === 'error' ? 'soft' : at === 0 ? 'solid' : 'outline'"
          :disabled="choice.value === 'ok' && blocked"
          @click="answer(choice.value)"
        />
      </div>
    </template>
  </UModal>
</template>
