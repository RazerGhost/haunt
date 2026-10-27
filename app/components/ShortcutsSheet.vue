<script setup lang="ts">
/*
 * Every keyboard shortcut of the window, by where it works: ? opens it (not while typing), and the command palette.
 * The list is written here by hand from the defineShortcuts and keydown handlers in app/, so a new shortcut belongs
 * here too. Keys are written as defineShortcuts does: meta is Ctrl, or ⌘ on macOS.
 */
const open = useState('shortcuts-sheet', () => false)
const { mac } = useDesktop()

defineShortcuts({ '?': () => { open.value = true } })

/** Each shortcut: its text (ShortcutsSheet.item.<id>) and its keys, one or more ways to press it. */
const GROUPS: { id: string, items: { id: string, keys: string[][] }[] }[] = [
  {
    id: 'window',
    items: [
      { id: 'palette', keys: [['meta', 'k']] },
      { id: 'settings', keys: [['meta', ',']] },
      { id: 'repositories', keys: [['meta', 'o']] },
      { id: 'reopen', keys: [['meta', 'shift', 't']] },
      { id: 'back', keys: [['alt', 'arrowleft']] },
      { id: 'forward', keys: [['alt', 'arrowright']] },
      { id: 'shortcuts', keys: [['?']] },
      { id: 'close', keys: [['escape']] },
      { id: 'managerMove', keys: [['arrowup'], ['arrowdown']] },
      { id: 'managerOpen', keys: [['enter']] },
      { id: 'managerBack', keys: [['escape']] }
    ]
  },
  {
    id: 'git',
    items: [
      { id: 'gitMode', keys: [['meta', '1']] },
      { id: 'fetch', keys: [['f5']] },
      { id: 'pull', keys: [['meta', 'shift', 'p']] },
      { id: 'push', keys: [['meta', 'shift', 'u']] }
    ]
  },
  {
    id: 'local',
    items: [
      { id: 'localMove', keys: [['arrowup'], ['arrowdown']] },
      { id: 'stage', keys: [['space']] },
      { id: 'commit', keys: [['meta', 'enter']] }
    ]
  },
  {
    id: 'commits',
    items: [
      { id: 'commitMove', keys: [['arrowup'], ['arrowdown']] },
      { id: 'compare', keys: [['meta', 'click']] }
    ]
  },
  {
    id: 'code',
    items: [
      { id: 'codeMode', keys: [['meta', '2']] },
      { id: 'find', keys: [['meta', 'f']] },
      { id: 'reviewNext', keys: [['arrowright'], ['j']] },
      { id: 'reviewPrevious', keys: [['arrowleft'], ['k']] }
    ]
  }
]

const SYMBOLS: Record<string, string> = { arrowleft: '←', arrowright: '→', arrowup: '↑', arrowdown: '↓', escape: 'Esc' }
function label(key: string): string {
  if (key === 'meta') return mac ? '⌘' : 'Ctrl'
  if (key === 'alt') return mac ? '⌥' : 'Alt'
  if (key === 'shift') return mac ? '⇧' : 'Shift'
  if (key === 'enter') return mac ? '↵' : 'Enter'
  if (key === 'space') return t('ShortcutsSheet.space')
  if (key === 'click') return t('ShortcutsSheet.click')
  return SYMBOLS[key] ?? key.toUpperCase()
}
</script>

<template>
  <UModal v-model:open="open" :title="t('ShortcutsSheet.title')" :description="t('ShortcutsSheet.description')" :ui="{ content: 'max-w-3xl' }">
    <template #body>
      <div class="grid gap-x-8 gap-y-5 sm:grid-cols-2">
        <section v-for="group in GROUPS" :key="group.id">
          <h3 class="mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted">{{ t(`ShortcutsSheet.group.${group.id}`) }}</h3>
          <dl class="grid gap-1.5">
            <div v-for="item in group.items" :key="item.id" class="flex items-center justify-between gap-3 text-sm">
              <dt class="min-w-0 text-default">{{ t(`ShortcutsSheet.item.${item.id}`) }}</dt>
              <dd class="flex shrink-0 items-center gap-1">
                <template v-for="(combo, at) in item.keys" :key="at">
                  <span v-if="at" class="text-xs text-dimmed">{{ t('ShortcutsSheet.or') }}</span>
                  <UKbd v-for="key in combo" :key="key" :value="label(key)" />
                </template>
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </template>
  </UModal>
</template>
