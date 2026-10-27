import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { test } from 'node:test'

/*
 * The window's texts (app/locales, looked up by app/utils/text.ts): every language has the same texts as English with
 * the same {params}, and every key the code names is there.
 */
type Texts = { [key: string]: string | Texts }

const app = join(import.meta.dirname, '..', 'app')
const locales = join(app, 'locales')

function files(folder: string, ending: RegExp): string[] {
  return readdirSync(folder, { recursive: true, encoding: 'utf8' })
    .filter(path => ending.test(path))
    .map(path => join(folder, path))
}

/** A language's texts by full key; a plural is one text whose forms are joined, to compare its params. */
function load(language: string): Map<string, { text: string, plural: boolean }> {
  const out = new Map<string, { text: string, plural: boolean }>()
  const folder = join(locales, language)
  for (const path of files(folder, /\.json$/)) {
    const prefix = relative(folder, path).replace(/\.json$/, '').replaceAll('\\', '/').replaceAll('/', '.')
    const walk = (at: string, texts: Texts): void => {
      for (const [key, value] of Object.entries(texts)) {
        if (typeof value === 'string') out.set(`${at}.${key}`, { text: value, plural: false })
        else if ('other' in value) out.set(`${at}.${key}`, { text: Object.values(value).join(' '), plural: true })
        else walk(`${at}.${key}`, value)
      }
    }
    walk(prefix, JSON.parse(readFileSync(path, 'utf8')) as Texts)
  }
  return out
}

const params = (text: string) => [...new Set(text.match(/\{\w+\}/g) ?? [])].sort()
const en = load('en')

for (const language of readdirSync(locales).filter(name => name !== 'en')) {
  test(`${language} has the texts English has, with the same params`, () => {
    const other = load(language)
    assert.deepEqual([...other.keys()].filter(key => !en.has(key)), [], 'texts English does not have')
    assert.deepEqual([...en.keys()].filter(key => !other.has(key)), [], 'texts missing')
    for (const [key, { text, plural }] of en) {
      const theirs = other.get(key)!
      assert.equal(theirs.plural, plural, `${key}: plural in one language only`)
      assert.deepEqual(params(theirs.text), params(text), `${key}: other params`)
    }
  })
}

test('every key the window names is in English', () => {
  const missing: string[] = []
  for (const path of files(app, /\.(vue|ts)$/)) {
    if (path.startsWith(locales)) continue
    for (const [, key] of readFileSync(path, 'utf8').matchAll(/\bt\(\s*'([\w.]+)'/g)) {
      if (!en.has(key!)) missing.push(`${relative(app, path)}: ${key}`)
    }
  }
  assert.deepEqual(missing, [])
})
