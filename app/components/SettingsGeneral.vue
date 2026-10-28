<script setup lang="ts">
import { EDITORS, type Editor } from '~~/shared/types'

/* Settings, General: the language, test files, the editor, commit times and order, diffs and the text size, kept in the window. */
const { includeTests, editor, textSize, times, order, diffWrap, tabWidth, ignoreSpace } = usePreferences()
const { app, save } = useAppSettings()
const language = useLanguage()
const languages = LANGUAGES.map(value => ({ value, label: LANGUAGE_NAMES[value] }))

const editorLabels = computed<Record<Editor, string>>(() => ({
  vscode: 'VS Code', vscodium: 'VSCodium', cursor: 'Cursor', windsurf: 'Windsurf', zed: 'Zed', webstorm: 'WebStorm', sublime: 'Sublime Text', kate: 'Kate', custom: t('SettingsGeneral.customEditor')
}))
// Installed editors first; the others stay pickable (their tool may be installed later).
const installed = ref<Editor[]>([])
async function findEditors(): Promise<void> {
  try {
    installed.value = await haunt.editors()
  } catch { /* none found: every editor stays pickable */ }
}
onMounted(findEditors)

// The custom editor's and the terminal's commands, kept by Rust; saved when a field is left.
const editorCommand = ref('')
const editorInTerminal = ref(false)
const terminalCommand = ref('')
watch(app, (settings) => {
  if (!settings) return
  editorCommand.value = settings.editorCommand
  editorInTerminal.value = settings.editorInTerminal
  terminalCommand.value = settings.terminalCommand
}, { immediate: true })
async function saveTools(): Promise<void> {
  const settings = app.value
  if (settings && editorCommand.value === settings.editorCommand && editorInTerminal.value === settings.editorInTerminal && terminalCommand.value === settings.terminalCommand) return
  if (await save(haunt.setTools(editorCommand.value, editorInTerminal.value, terminalCommand.value))) await findEditors()
}
const placeholders = { file: '{file}', line: '{line}', column: '{column}', root: '{root}', folder: '{folder}', command: '{command}' }
const editors = computed(() => EDITORS
  .map(value => ({
    value,
    label: installed.value.includes(value) ? editorLabels.value[value] : t('SettingsGeneral.notFound', { editor: editorLabels.value[value] }),
    found: installed.value.includes(value)
  }))
  .sort((a, b) => Number(b.found) - Number(a.found)))
watch(installed, (found) => {
  if (found.length && !found.includes(editor.value)) editor.value = found[0]!
})
const sizes = ['small', 'normal', 'large'] as const
</script>

<template>
  <div class="grid content-start gap-5">
    <UFormField :label="t('SettingsGeneral.language')">
      <USelect v-model="language" :items="languages" icon="i-lucide-languages" class="w-48" />
    </UFormField>
    <USwitch v-model="includeTests" :label="t('SettingsGeneral.showTests')" :description="t('SettingsGeneral.showTestsHelp')" />
    <UFormField :label="t('SettingsGeneral.editor')" :help="installed.length ? undefined : t('SettingsGeneral.noEditor')">
      <USelect v-model="editor" :items="editors" icon="i-lucide-square-pen" class="w-72" />
    </UFormField>
    <div v-if="editor === 'custom'" class="grid gap-2">
      <UFormField :label="t('SettingsGeneral.customCommand')" :help="t('SettingsGeneral.customCommandHelp', placeholders)">
        <UInput v-model="editorCommand" placeholder="nvim +{line} {file}" class="w-96 max-w-full font-mono" @change="saveTools" />
      </UFormField>
      <USwitch v-model="editorInTerminal" :label="t('SettingsGeneral.inTerminal')" :description="t('SettingsGeneral.inTerminalHelp')" @update:model-value="saveTools" />
    </div>
    <UFormField :label="t('SettingsGeneral.terminal')" :help="t('SettingsGeneral.terminalHelp', placeholders)">
      <UInput v-model="terminalCommand" :placeholder="t('SettingsGeneral.terminalFound')" icon="i-lucide-square-terminal" class="w-96 max-w-full font-mono" @change="saveTools" />
    </UFormField>
    <UFormField :label="t('SettingsGeneral.times')" :help="times === 'ago' ? t('SettingsGeneral.timesAgoHelp') : t('SettingsGeneral.timesDateHelp')">
      <UFieldGroup>
        <UButton :label="t('SettingsGeneral.timesAgo')" size="sm" :color="times === 'ago' ? 'primary' : 'neutral'" :variant="times === 'ago' ? 'solid' : 'outline'" @click="times = 'ago'" />
        <UButton :label="t('SettingsGeneral.timesDate')" size="sm" :color="times === 'date' ? 'primary' : 'neutral'" :variant="times === 'date' ? 'solid' : 'outline'" @click="times = 'date'" />
      </UFieldGroup>
    </UFormField>
    <UFormField :label="t('SettingsGeneral.order')" :help="order === 'topology' ? t('SettingsGeneral.orderTopologyHelp') : t('SettingsGeneral.orderDateHelp')">
      <UFieldGroup>
        <UButton :label="t('SettingsGeneral.orderTopology')" size="sm" :color="order === 'topology' ? 'primary' : 'neutral'" :variant="order === 'topology' ? 'solid' : 'outline'" @click="order = 'topology'" />
        <UButton :label="t('SettingsGeneral.orderDate')" size="sm" :color="order === 'date' ? 'primary' : 'neutral'" :variant="order === 'date' ? 'solid' : 'outline'" @click="order = 'date'" />
      </UFieldGroup>
    </UFormField>
    <UFormField :label="t('SettingsGeneral.diffs')">
      <div class="grid gap-2">
        <USwitch v-model="diffWrap" :label="t('SettingsGeneral.wrap')" />
        <USwitch v-model="ignoreSpace" :label="t('SettingsGeneral.ignoreSpace')" :description="t('SettingsGeneral.ignoreSpaceHelp')" />
        <div class="flex items-center gap-2 text-sm">
          {{ t('SettingsGeneral.tabBefore') }}
          <UFieldGroup size="xs">
            <UButton v-for="width in [2, 4, 8] as const" :key="width" :label="String(width)" :color="tabWidth === width ? 'primary' : 'neutral'" :variant="tabWidth === width ? 'solid' : 'outline'" @click="tabWidth = width" />
          </UFieldGroup>
          {{ t('SettingsGeneral.tabAfter') }}
        </div>
      </div>
    </UFormField>
    <UFormField :label="t('SettingsGeneral.textSize')" :help="t('SettingsGeneral.textSizeHelp')">
      <UFieldGroup>
        <UButton
          v-for="size in sizes"
          :key="size"
          :label="t(`SettingsGeneral.size.${size}`)"
          size="sm"
          :color="textSize === size ? 'primary' : 'neutral'"
          :variant="textSize === size ? 'solid' : 'outline'"
          @click="textSize = size"
        />
      </UFieldGroup>
    </UFormField>
  </div>
</template>
