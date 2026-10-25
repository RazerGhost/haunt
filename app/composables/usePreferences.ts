import { EDITORS, type Editor } from '~~/shared/types'

/*
 * The window's settings that more than one screen uses, kept between launches: whether test files show, the editor
 * functions open in, the text size, whether the commit box starts with sign-off, how the commit list shows times and in
 * what order it lists commits, and the folders whose trust banner was
 * answered with "Not now" (the shield in the toolbar can ask again). Settings (the gear) changes them.
 */
export type TextSize = 'small' | 'normal' | 'large'
/** Each text size, in px from the theme's root font size (16 unless the theme says otherwise); everything in rem follows. */
export const TEXT_STEPS: Record<TextSize, number> = {
  small: -2,
  normal: 0,
  large: 2
}

export function usePreferences() {
  /** Load a theme's fonts from coolLabs Fonts (Haunt goes online for them); off, a font is used only when installed. */
  const remoteFonts = usePersisted('remote-fonts', false, value => typeof value === 'boolean')
  const includeTests = usePersisted('tests', false, value => typeof value === 'boolean')
  const editor = usePersisted<Editor>('editor', 'vscode', value => EDITORS.includes(value as Editor))
  const trustDismissed = usePersisted<string[]>('trust-dismissed', [], value => Array.isArray(value) && value.every(path => typeof path === 'string'))
  const textSize = usePersisted<TextSize>('text-size', 'normal', value => value === 'small' || value === 'normal' || value === 'large')
  const signoff = usePersisted('signoff', false, value => typeof value === 'boolean')
  /** Commit times as how long ago ("8 min") or as the date and time. */
  const times = usePersisted<'ago' | 'date'>('commit-times', 'ago', value => value === 'ago' || value === 'date')
  /** The commit list by topology (a merged branch's commits together) or by date. */
  const order = usePersisted<'topology' | 'date'>('commit-order', 'topology', value => value === 'topology' || value === 'date')
  /** Diffs: long lines wrapped, a tab's width, and (in commit diffs) changes of whitespace only left out. */
  const diffWrap = usePersisted('diff-wrap', false, value => typeof value === 'boolean')
  const tabWidth = usePersisted<2 | 4 | 8>('tab-width', 4, value => value === 2 || value === 4 || value === 8)
  const ignoreSpace = usePersisted('ignore-space', false, value => typeof value === 'boolean')
  return { remoteFonts, includeTests, editor, trustDismissed, textSize, signoff, times, order, diffWrap, tabWidth, ignoreSpace }
}
