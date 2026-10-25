// https://nuxt.com/docs/api/configuration/nuxt-config
import { PRESET_LOOKS } from './shared/themes'

export default defineNuxtConfig({
  modules: ['@nuxt/ui'],

  // The window of a desktop app: built to static files the app loads itself (`nuxt generate`); it talks to Haunt's
  // Rust side, never to a server of its own.
  ssr: false,

  // Routes after a #, so a reload in the app's window finds its page among the built files.
  router: { options: { hashMode: true } },

  // Haunt is itself the inspector: no DevTools panel over it.
  devtools: { enabled: false },

  css: ['~/assets/css/main.css'],

  // The icons are the Lucide ones the code names and the themes show, bundled into the window; the other icon sets a theme
  // may use are downloaded on request and kept by the app (useIconSets). Fonts are not bundled: a theme's font is used when it is installed
  // on the computer, else the system's, or loaded from coolLabs Fonts when Settings, Appearance says so (useAppearance).
  ui: { fonts: false },
  icon: {
    provider: 'none',
    fallbackToApi: false,
    serverBundle: false,
    clientBundle: {
      scan: true,
      icons: Object.values(PRESET_LOOKS).map(look => look.icon.replace(/^i-lucide-/, 'lucide:'))
    }
  },

  devServer: { host: '127.0.0.1', port: 4777 },

  // While Haunt is worked on in a browser, its calls go to the app's development bridge (src-tauri/src/devbridge.rs).
  nitro: { devProxy: { '/__haunt': { target: 'http://127.0.0.1:4778/__haunt' } } },

  // The generated bindings (app/bindings.ts) import Tauri's invoke; bundled up front, so the dev server does not reload for it.
  vite: { optimizeDeps: { include: ['@tauri-apps/api/core'] } },

  compatibilityDate: '2026-06-30',

  typescript: {
    strict: true,
    tsConfig: { compilerOptions: { noUncheckedIndexedAccess: true, allowImportingTsExtensions: true } }
  }
})
