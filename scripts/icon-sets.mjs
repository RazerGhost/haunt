// Writes shared/iconSets.json: for each icon set a theme may pick, the name there of every Lucide icon Haunt and Nuxt UI
// use. A name is the same one where the set has it, else the first synonym it has (below); an icon with no match in a
// set stays Lucide there. Run again (`node scripts/icon-sets.mjs`) after adding an icon; shared/iconSets.test.ts checks
// that every icon the window names is in the file and every name in it is in its set.
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')

/** The sets, by the name a theme gives them, with their Iconify prefix and how a name is tried there. */
const SETS = {
  bootstrap: { prefix: 'bi', forms: name => [name] },
  phosphor: { prefix: 'ph', forms: name => [name] },
  tabler: { prefix: 'tabler', forms: name => [name] },
  remix: { prefix: 'ri', forms: name => [`${name}-line`, name] },
  material: { prefix: 'material-symbols', forms: name => [`${name}-outline`, name, `${name}-rounded`] },
  heroicons: { prefix: 'heroicons', forms: name => [name] },
  iconoir: { prefix: 'iconoir', forms: name => [name] }
}

/** Other names a Lucide icon goes by in the other sets, tried in order after its own. */
const SYNONYMS = {
  'x': ['x-mark', 'xmark', 'close', 'x-lg'],
  'search': ['magnifying-glass', 'search'],
  'text-search': ['file-search', 'text-search', 'search-text', 'magnifying-glass', 'search'],
  'loader-circle': ['loader-2', 'spinner', 'circle-notch', 'progress-activity', 'loader', 'arrow-clockwise', 'arrow-path', 'refresh-double', 'loader-4'],
  'refresh-cw': ['refresh', 'arrow-clockwise', 'arrow-path', 'arrow-repeat', 'refresh-double', 'sync'],
  'rotate-ccw': ['rotate', 'arrow-counter-clockwise', 'arrow-uturn-left', 'arrow-counterclockwise', 'undo', 'reset', 'restart'],
  'undo-2': ['arrow-back-up', 'arrow-uturn-left', 'arrow-u-up-left', 'undo', 'arrow-go-back', 'reply'],
  'trash-2': ['trash', 'trash-simple', 'delete-bin', 'delete', 'bin'],
  'pencil': ['pencil', 'pencil-simple', 'pen', 'edit', 'edit-pencil'],
  'pencil-line': ['pencil-line', 'pencil-simple-line', 'edit', 'edit-pencil', 'pencil-square'],
  'square-pen': ['edit', 'pencil-square', 'note-pencil', 'edit-box', 'pencil-square', 'edit-square'],
  'settings-2': ['settings', 'gear', 'adjustments', 'cog-6-tooth', 'gear-six', 'settings-3'],
  'sliders-horizontal': ['adjustments-horizontal', 'sliders-horizontal', 'sliders', 'equalizer', 'tune', 'settings-adjust', 'settings'],
  'circle-alert': ['alert-circle', 'warning-circle', 'exclamation-circle', 'error-warning', 'error', 'info-circle'],
  'triangle-alert': ['alert-triangle', 'warning', 'exclamation-triangle', 'warning-triangle', 'error-warning'],
  'circle-check': ['circle-check', 'check-circle', 'checkbox-circle', 'check-circle-fill'],
  'check-circle': ['circle-check', 'check-circle', 'checkbox-circle'],
  'circle-x': ['circle-x', 'x-circle', 'close-circle', 'xmark-circle', 'cancel'],
  'check': ['check', 'check-lg'],
  'chevron-down': ['chevron-down', 'caret-down', 'arrow-down-s', 'nav-arrow-down', 'expand-more', 'keyboard-arrow-down'],
  'chevron-up': ['chevron-up', 'caret-up', 'arrow-up-s', 'nav-arrow-up', 'expand-less', 'keyboard-arrow-up'],
  'chevron-left': ['chevron-left', 'caret-left', 'arrow-left-s', 'nav-arrow-left', 'keyboard-arrow-left'],
  'chevron-right': ['chevron-right', 'caret-right', 'arrow-right-s', 'nav-arrow-right', 'keyboard-arrow-right'],
  'chevrons-left': ['chevrons-left', 'chevron-double-left', 'caret-double-left', 'arrow-left-double', 'keyboard-double-arrow-left', 'fast-arrow-left'],
  'chevrons-right': ['chevrons-right', 'chevron-double-right', 'caret-double-right', 'arrow-right-double', 'keyboard-double-arrow-right', 'fast-arrow-right'],
  'ellipsis': ['dots', 'dots-three', 'three-dots', 'ellipsis-horizontal', 'more', 'more-horiz', 'more-horiz-circle'],
  'grip-vertical': ['grip-vertical', 'dots-six-vertical', 'drag-move-2', 'draggable', 'drag-indicator', 'drag', 'ellipsis-vertical'],
  'menu': ['menu', 'list', 'menu-2', 'bars-3', 'menu-scale'],
  'copy': ['copy', 'clipboard', 'document-duplicate', 'file-copy', 'content-copy'],
  'copy-check': ['copy-check', 'clipboard-check', 'check-circle', 'file-copy-2', 'checks'],
  'copy-x': ['copy-x', 'clipboard-x', 'x-square', 'close-circle', 'content-copy', 'x-circle'],
  'external-link': ['external-link', 'box-arrow-up-right', 'arrow-square-out', 'arrow-top-right-on-square', 'open-new-window', 'open-in-new'],
  'square-arrow-out-up-right': ['external-link', 'box-arrow-up-right', 'arrow-square-out', 'arrow-top-right-on-square', 'open-new-window', 'open-in-new'],
  'arrow-up-right': ['arrow-up-right', 'arrow-up-right-circle', 'north-east', 'arrow-right-up'],
  'eye-off': ['eye-off', 'eye-slash', 'eye-closed', 'visibility-off'],
  'eye': ['eye', 'visibility'],
  'file': ['file', 'document', 'page', 'description'],
  'files': ['files', 'file-stack', 'document-duplicate', 'multiple-pages', 'file-copy'],
  'file-diff': ['file-diff', 'git-compare', 'file-earmark-diff', 'git-diff', 'difference', 'compare', 'file-text', 'document-text'],
  'folder-git-2': ['folder-git', 'folder-code', 'folder-simple', 'folder', 'git-repository', 'git-fork'],
  'folder-x': ['folder-x', 'folder-minus', 'folder-off', 'folder-forbid', 'folder-warning', 'folder-close'],
  'folder-open': ['folder-open', 'folder-2-open', 'folder2-open', 'open-book'],
  'git-branch': ['git-branch', 'branch', 'git-fork', 'git', 'account-tree'],
  'git-branch-plus': ['git-branch-plus', 'git-branch', 'git-fork', 'add-branch', 'git', 'account-tree'],
  'git-commit-horizontal': ['git-commit', 'commit', 'git'],
  'git-commit-vertical': ['git-commit', 'commit', 'git'],
  'git-fork': ['git-fork', 'fork', 'git-branch', 'git', 'account-tree'],
  'git-merge': ['git-merge', 'merge', 'merge-cell', 'git', 'merge'],
  'git-pull-request': ['git-pull-request', 'pull-request', 'git', 'merge'],
  'git-pull-request-arrow': ['git-pull-request', 'pull-request', 'git-pull-request-draft', 'git', 'merge'],
  'history': ['history', 'clock-counter-clockwise', 'clock-history', 'clock', 'time'],
  'archive': ['archive', 'archive-box', 'box', 'inventory'],
  'archive-restore': ['archive-restore', 'archive-box-arrow-down', 'box-arrow-up', 'unarchive', 'archive-off', 'arrow-up-on-square', 'archive-stack', 'archive'],
  'archive-x': ['archive-x', 'archive-box-x-mark', 'archive-off', 'box-arrow-in-down', 'delete-bin', 'archive'],
  'arrow-down-to-line': ['arrow-bar-to-down', 'arrow-line-down', 'download', 'arrow-down-tray', 'vertical-align-bottom'],
  'arrow-up-from-line': ['arrow-bar-up', 'arrow-line-up', 'upload', 'arrow-up-tray', 'vertical-align-top'],
  'arrow-right-to-line': ['arrow-bar-to-right', 'arrow-line-right', 'arrow-right-end', 'arrow-bar-right', 'last-page', 'align-right', 'arrow-right-end-on-rectangle'],
  'cloud-download': ['cloud-download', 'cloud-arrow-down', 'download-cloud', 'cloud-download-alt'],
  'boxes': ['boxes', 'box-multiple', 'cube', 'squares-four', 'view-grid', 'layout-grid', 'apps'],
  'braces': ['braces', 'brackets-curly', 'code-bracket', 'code', 'data-object', 'curly-brackets'],
  'square-function': ['math-function', 'function', 'function-square', 'variable', 'functions', 'calculator'],
  'cherry': ['cherry', 'cherries', 'git-cherry-pick', 'apple', 'heart'],
  'dot': ['dot', 'circle-small', 'point', 'circle-fill', 'record', 'fiber-manual-record', 'circle'],
  'eraser': ['eraser', 'clear-formatting', 'ink-eraser', 'erase', 'backspace'],
  'filter': ['filter', 'funnel', 'filter-alt', 'funnel-simple'],
  'layers': ['layers', 'stack', 'stack-2', 'square-3-stack-3d', 'layers-subtract', 'square-3-stack-3d', 'box'],
  'list-checks': ['list-checks', 'list-check', 'checklist', 'list-todo', 'task', 'queue-list', 'list'],
  'maximize': ['maximize', 'arrows-out', 'arrows-maximize', 'fullscreen', 'arrows-pointing-out', 'expand'],
  'message-square': ['message', 'chat', 'chat-square', 'chat-bubble', 'message-2', 'chat-left', 'chat-bubble-left'],
  'move-right': ['arrow-right', 'arrow-narrow-right', 'arrow-long-right', 'move-right'],
  'network': ['network', 'sitemap', 'share-network', 'git-network', 'graph', 'hierarchy', 'diagram-3', 'account-tree', 'share'],
  'package-open': ['package', 'box-open', 'package-2', 'archive', 'cube', 'box'],
  'palette': ['palette', 'color-swatch', 'swatches', 'brush', 'swatch'],
  'pin': ['pin', 'push-pin', 'pushpin', 'map-pin'],
  'pin-off': ['pin-off', 'push-pin-slash', 'unpin', 'pushpin-slash', 'pin-slash', 'pinned-off', 'pin-angle', 'map-pin'],
  'play': ['play', 'play-circle', 'player-play'],
  'replace': ['replace', 'arrows-exchange', 'arrows-left-right', 'swap', 'arrows-right-left', 'repeat'],
  'shield': ['shield', 'shield-check'],
  'shield-alert': ['shield-exclamation', 'shield-warning', 'shield-alert', 'shield-exclamation', 'shield'],
  'shield-check': ['shield-check', 'shield-checkered', 'shield-tick', 'verified-user'],
  'shield-question': ['shield-question', 'shield-help', 'shield-search', 'question-circle', 'shield-check'],
  'tag': ['tag', 'label', 'price-tag'],
  'user-round': ['user', 'user-circle', 'person', 'account-circle'],
  'user-round-plus': ['user-plus', 'user-add', 'person-add', 'user-circle-plus'],
  'user-round-search': ['user-search', 'user-scan', 'person-search', 'user-circle', 'person'],
  'user': ['user', 'person'],
  'lightbulb': ['bulb', 'lightbulb', 'light-bulb', 'lightbulb-filament'],
  'hash': ['hash', 'hashtag', 'number', 'tag'],
  'panel-left-close': ['layout-sidebar-left-collapse', 'sidebar-collapse', 'sidebar-simple', 'layout-left-line', 'left-panel-close', 'layout-sidebar', 'arrow-left-start-on-rectangle'],
  'panel-left-open': ['layout-sidebar-left-expand', 'sidebar-expand', 'sidebar-simple', 'layout-left-line', 'left-panel-open', 'layout-sidebar-inset', 'arrow-right-end-on-rectangle'],
  'terminal': ['terminal', 'terminal-window', 'command-line', 'terminal-box'],
  'upload': ['upload', 'arrow-up-tray', 'cloud-upload'],
  'minus': ['minus', 'dash', 'subtract', 'remove'],
  'plus': ['plus', 'add'],
  'square': ['square', 'stop'],
  'star': ['star'],
  'sun': ['sun', 'sun-light', 'light-mode', 'brightness-high'],
  'moon': ['moon', 'half-moon', 'dark-mode', 'moon-stars'],
  'monitor': ['device-desktop', 'desktop', 'monitor', 'computer-desktop', 'computer', 'pc-display'],
  'info': ['info-circle', 'info', 'information', 'information-circle'],
  'bell': ['bell', 'notifications'],
  'cloud': ['cloud'],
  'crosshair': ['crosshair', 'target', 'focus', 'gps-fixed', 'crosshair-simple', 'cursor-arrow-rays', 'my-location', 'frame-select'],
  'arrow-down': ['arrow-down', 'arrow-downward'],
  'arrow-up': ['arrow-up', 'arrow-upward'],
  'arrow-left': ['arrow-left'],
  'arrow-right': ['arrow-right']
}

function walk(dir, out) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) walk(path, out)
    else if (/\.(vue|ts)$/.test(name)) out.push(path)
  }
  return out
}

// The Lucide icons named in the window, and the ones Nuxt UI's own components use.
const named = new Set()
for (const file of walk(join(root, 'app'), [])) {
  for (const [match] of readFileSync(file, 'utf8').matchAll(/i-lucide-[a-z0-9-]+/g)) named.add(match.slice('i-lucide-'.length))
}
const uiDir = join(root, 'node_modules', '@nuxt', 'ui', 'dist')
for (const file of walk(uiDir, []).concat(readdirSync(join(uiDir, 'shared')).map(name => join(uiDir, 'shared', name)), [join(uiDir, 'module.mjs')])) {
  if (!/\.(mjs|ts|vue)$/.test(file)) continue
  for (const [match] of readFileSync(file, 'utf8').matchAll(/i-lucide-[a-z0-9-]+/g)) named.add(match.slice('i-lucide-'.length))
}

const out = {}
const missing = {}
for (const [set, { prefix, forms }] of Object.entries(SETS)) {
  const icons = JSON.parse(readFileSync(join(root, 'node_modules', '@iconify-json', prefix, 'icons.json'), 'utf8'))
  const has = name => !!icons.icons[name] || !!icons.aliases?.[name]
  out[set] = {}
  missing[set] = []
  for (const lucide of [...named].sort()) {
    const candidates = [lucide, ...(SYNONYMS[lucide] ?? [])]
    const found = candidates.flatMap(forms).find(has)
    if (found) out[set][`lucide-${lucide}`] = `${prefix}:${found}`
    else missing[set].push(lucide)
  }
}
writeFileSync(join(root, 'shared', 'iconSets.json'), `${JSON.stringify({ named: [...named].sort(), sets: out }, null, 2)}\n`)
for (const [set, list] of Object.entries(missing)) console.log(`${set}: ${named.size - list.length}/${named.size}${list.length ? ` (stay Lucide: ${list.join(', ')})` : ''}`)
