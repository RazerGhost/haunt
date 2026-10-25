import type { HighlighterCore, ThemedToken } from 'shiki/core'

/*
 * Syntax colours for code in the diff viewer and the function panel, with Shiki: one highlighter for the app, the
 * JavaScript regex engine (no WebAssembly), and each language's grammar loaded the first time a file of it shows.
 * Tokens carry both themes' colours as CSS variables (--shiki-light, --shiki-dark); main.css picks one.
 */

const GRAMMARS: Record<string, () => Promise<unknown>> = {
  typescript: () => import('shiki/langs/typescript.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  javascript: () => import('shiki/langs/javascript.mjs'),
  jsx: () => import('shiki/langs/jsx.mjs'),
  vue: () => import('shiki/langs/vue.mjs'),
  svelte: () => import('shiki/langs/svelte.mjs'),
  astro: () => import('shiki/langs/astro.mjs'),
  php: () => import('shiki/langs/php.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
  lua: () => import('shiki/langs/lua.mjs'),
  python: () => import('shiki/langs/python.mjs'),
  go: () => import('shiki/langs/go.mjs'),
  shellscript: () => import('shiki/langs/shellscript.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  scss: () => import('shiki/langs/scss.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
  toml: () => import('shiki/langs/toml.mjs')
}

const EXTENSIONS: [RegExp, string][] = [
  [/\.[cm]?ts$/, 'typescript'], [/\.tsx$/, 'tsx'], [/\.[cm]?js$/, 'javascript'], [/\.jsx$/, 'jsx'],
  [/\.vue$/, 'vue'], [/\.svelte$/, 'svelte'], [/\.astro$/, 'astro'], [/\.php$/, 'php'], [/\.rs$/, 'rust'],
  [/\.lua$/, 'lua'], [/\.py$/, 'python'], [/\.go$/, 'go'], [/\.(sh|bash)$/, 'shellscript'], [/\.jsonc?$/, 'json'], [/\.css$/, 'css'], [/\.scss$/, 'scss'], [/\.html?$/, 'html'],
  [/\.mdx?$/, 'markdown'], [/\.ya?ml$/, 'yaml'], [/\.toml$/, 'toml']
]

/** Past this many lines, code shows without colours: tokenizing it would hold up the window. */
const MAX_LINES = 4000

let highlighter: Promise<HighlighterCore> | null = null
const loaded = new Set<string>()

function create(): Promise<HighlighterCore> {
  highlighter ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([import('shiki/core'), import('shiki/engine/javascript')])
    return createHighlighterCore({
      themes: [import('shiki/themes/github-light.mjs'), import('shiki/themes/github-dark.mjs')],
      langs: [],
      // Forgiving: a pattern the JavaScript engine cannot run leaves that bit uncoloured instead of failing the file.
      engine: createJavaScriptRegexEngine({ forgiving: true })
    })
  })()
  return highlighter
}

function languageOfPath(path: string): string | null {
  return EXTENSIONS.find(([pattern]) => pattern.test(path))?.[1] ?? null
}

/**
 * The lines' tokens, one list per line; null for a language Haunt has no grammar for, or too many lines. `script`: the
 * lines are code from a Vue, Svelte or Astro file's script, coloured as TypeScript rather than as markup.
 */
export async function highlight(path: string, lines: string[], { script = false } = {}): Promise<ThemedToken[][] | null> {
  const found = languageOfPath(path)
  const lang = script && found && ['vue', 'svelte', 'astro'].includes(found) ? 'typescript' : found
  if (!lang || !lines.length || lines.length > MAX_LINES) return null
  try {
    const shiki = await create()
    if (!loaded.has(lang)) {
      await shiki.loadLanguage(await GRAMMARS[lang]!() as never)
      loaded.add(lang)
    }
    return shiki.codeToTokens(lines.join('\n'), { lang, themes: { light: 'github-light', dark: 'github-dark' }, defaultColor: false }).tokens
  } catch {
    return null
  }
}
