<script setup lang="ts">
import { EDITORS, type Editor } from '~~/shared/types'

/* Settings, General: test files, the editor, commit times and order, diffs and the text size, kept in the window. */
const { includeTests, editor, textSize, times, order, diffWrap, tabWidth, ignoreSpace } = usePreferences()

const editorLabels: Record<Editor, string> = {
  vscode: 'VS Code', cursor: 'Cursor', windsurf: 'Windsurf', zed: 'Zed', webstorm: 'WebStorm', sublime: 'Sublime Text', custom: 'Custom (HAUNT_EDITOR)'
}
// Installed editors first; the others stay pickable (their tool may be installed later).
const installed = ref<Editor[]>([])
onMounted(async () => {
  try {
    installed.value = await haunt.editors()
  } catch { /* none found: every editor stays pickable */ }
})
const editors = computed(() => EDITORS
  .map(value => ({ value, label: installed.value.includes(value) ? editorLabels[value] : `${editorLabels[value]} (not found)`, found: installed.value.includes(value) }))
  .sort((a, b) => Number(b.found) - Number(a.found)))
watch(installed, (found) => {
  if (found.length && !found.includes(editor.value)) editor.value = found[0]!
})
const sizes = [
  { value: 'small', label: 'Small' },
  { value: 'normal', label: 'Normal' },
  { value: 'large', label: 'Large' }
] as const
</script>

<template>
  <div class="grid content-start gap-5">
    <USwitch v-model="includeTests" label="Show test files" description="Tests and fixtures in the layers, the canvas, calls and search." />
    <UFormField label="Open functions in" :help="installed.length ? undefined : 'No editor found on your PATH.'">
      <USelect v-model="editor" :items="editors" icon="i-lucide-square-pen" class="w-72" />
    </UFormField>
    <UFormField label="Commit times" :help="times === 'ago' ? 'Such as 8 min or 3 d; hover for the date.' : 'The date and time of each commit.'">
      <UFieldGroup>
        <UButton label="How long ago" size="sm" :color="times === 'ago' ? 'primary' : 'neutral'" :variant="times === 'ago' ? 'solid' : 'outline'" @click="times = 'ago'" />
        <UButton label="Date and time" size="sm" :color="times === 'date' ? 'primary' : 'neutral'" :variant="times === 'date' ? 'solid' : 'outline'" @click="times = 'date'" />
      </UFieldGroup>
    </UFormField>
    <UFormField label="Commit order" :help="order === 'topology' ? 'A merged branch\'s commits stay under its merge.' : 'Newest first, one lane per open branch.'">
      <UFieldGroup>
        <UButton label="Branches together" size="sm" :color="order === 'topology' ? 'primary' : 'neutral'" :variant="order === 'topology' ? 'solid' : 'outline'" @click="order = 'topology'" />
        <UButton label="By date" size="sm" :color="order === 'date' ? 'primary' : 'neutral'" :variant="order === 'date' ? 'solid' : 'outline'" @click="order = 'date'" />
      </UFieldGroup>
    </UFormField>
    <UFormField label="Diffs">
      <div class="grid gap-2">
        <USwitch v-model="diffWrap" label="Wrap long lines" />
        <USwitch v-model="ignoreSpace" label="Ignore whitespace" description="In commit diffs. Local changes always shows them, since lines are staged from it." />
        <div class="flex items-center gap-2 text-sm">
          A tab is
          <UFieldGroup size="xs">
            <UButton v-for="width in [2, 4, 8] as const" :key="width" :label="String(width)" :color="tabWidth === width ? 'primary' : 'neutral'" :variant="tabWidth === width ? 'solid' : 'outline'" @click="tabWidth = width" />
          </UFieldGroup>
          spaces wide
        </div>
      </div>
    </UFormField>
    <UFormField label="Text size" help="The window's text; the canvas sizes its own names to its boxes.">
      <UFieldGroup>
        <UButton
          v-for="option in sizes"
          :key="option.value"
          :label="option.label"
          size="sm"
          :color="textSize === option.value ? 'primary' : 'neutral'"
          :variant="textSize === option.value ? 'solid' : 'outline'"
          @click="textSize = option.value"
        />
      </UFieldGroup>
    </UFormField>
  </div>
</template>
