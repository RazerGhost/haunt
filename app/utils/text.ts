/*
 * The window's texts, by language: `t('common.cancel')` looks a text up in app/locales/<language>/, where each file
 * is a namespace named after its path (CommitBox.json is `CommitBox`, pages/local.json is `pages.local`, common.json
 * is `common`). `{name}` in a text is filled from the params; a text written as `{ "one": …, "other": … }` is picked by
 * the `count` param under the language's plural rules. A text missing in a language falls back to English, then to its
 * key. The language is a window setting, kept between launches; it starts as the system's when Haunt has it.
 *
 * Texts Haunt writes into a repository (commit message drafts, release notes) stay English: they are the project's,
 * not the window's.
 */
export const LANGUAGES = ['en', 'nl'] as const
export type Language = typeof LANGUAGES[number]
/** Each language by its own name, for the setting. */
export const LANGUAGE_NAMES: Record<Language, string> = { en: 'English', nl: 'Nederlands' }

type Texts = { [key: string]: string | Texts }

const files = import.meta.glob<Texts>('../locales/*/**/*.json', { eager: true, import: 'default' })
const tables = Object.fromEntries(LANGUAGES.map(language => [language, new Map<string, string | Texts>()])) as Record<Language, Map<string, string | Texts>>
for (const [path, texts] of Object.entries(files)) {
  const [, language, file] = path.match(/^\.\.\/locales\/([^/]+)\/(.+)\.json$/) ?? []
  if (!language || !file || !LANGUAGES.includes(language as Language)) continue
  flatten(tables[language as Language], file.replaceAll('/', '.'), texts)
}

/** Every text under its full key; a plural's forms stay together under the plural's key. */
function flatten(into: Map<string, string | Texts>, prefix: string, texts: Texts): void {
  for (const [key, value] of Object.entries(texts)) {
    const full = `${prefix}.${key}`
    if (typeof value === 'string' || 'other' in value) into.set(full, value)
    else flatten(into, full, value)
  }
}

function systemLanguage(): Language {
  const asked = import.meta.client ? navigator.language.slice(0, 2).toLowerCase() : 'en'
  return LANGUAGES.includes(asked as Language) ? asked as Language : 'en'
}

export function useLanguage(): Ref<Language> {
  return usePersisted<Language>('language', systemLanguage(), value => LANGUAGES.includes(value as Language))
}

const plurals = new Map<Language, Intl.PluralRules>()

/** The text under `key` in the window's language, with its `{name}`s filled from `params`. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const language = useLanguage().value
  const found = tables[language].get(key) ?? tables.en.get(key)
  if (found === undefined) return key
  let text: string
  if (typeof found === 'string') {
    text = found
  } else {
    const count = Number(params.count ?? 0)
    if (!plurals.has(language)) plurals.set(language, new Intl.PluralRules(language))
    const form = count === 0 && typeof found.zero === 'string' ? 'zero' : plurals.get(language)!.select(count)
    const picked = found[form] ?? found.other
    text = typeof picked === 'string' ? picked : key
  }
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => name in params ? String(params[name]) : whole)
}
