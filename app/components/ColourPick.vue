<script setup lang="ts">
import { hexOf } from '~~/shared/ramp'

// Any colour, beside the named ones in Appearance: a round button that opens a colour picker and a hex field. The
// colour is chosen when the picker closes or Enter is pressed, not on every move of the picker (each choice is saved).
const props = defineProps<{ value: string | undefined, label: string }>()
const emit = defineEmits<{ pick: [hex: string] }>()

const chosen = computed(() => hexOf(props.value ?? ''))
const open = ref(false)
const draft = ref('#808080')
const typed = ref('')
watch(open, (now) => {
  if (now) {
    draft.value = chosen.value ?? '#808080'
    typed.value = draft.value
    return
  }
  const hex = hexOf(draft.value)
  if (hex && hex !== chosen.value) emit('pick', hex)
})
watch(draft, (value) => { typed.value = value })
function enter(): void {
  const hex = hexOf(typed.value)
  if (!hex) return
  draft.value = hex
  open.value = false
}
</script>

<template>
  <UPopover v-model:open="open">
    <button
      type="button"
      class="grid size-7 place-items-center rounded-full border-2"
      :class="chosen ? 'border-highlighted' : 'border-transparent hover:border-accented'"
      :title="label"
      :aria-label="label"
      :aria-pressed="!!chosen"
    >
      <span
        class="grid size-5 place-items-center rounded-full border border-default"
        :style="chosen ? { background: chosen } : { background: 'conic-gradient(#f43f5e, #f59e0b, #84cc16, #06b6d4, #6366f1, #d946ef, #f43f5e)' }"
      >
        <UIcon v-if="!chosen" name="i-lucide-pipette" class="size-3 text-white drop-shadow" />
      </span>
    </button>
    <template #content>
      <div class="grid gap-2 p-2">
        <UColorPicker v-model="draft" format="hex" />
        <UInput v-model="typed" size="sm" class="font-mono" :aria-label="label" @keydown.enter="enter" @blur="hexOf(typed) && (draft = hexOf(typed)!)" />
      </div>
    </template>
  </UPopover>
</template>
