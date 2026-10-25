import { commands } from '~/bindings'
import type { CallEdge, Editor, Scan, ScanEvent } from '~~/shared/types'

/*
 * The window's calls to Haunt's Rust side: the commands of app/bindings.ts, written from src-tauri/src/api.rs, so their
 * names, arguments and answers are typed from the Rust. A failed call throws Rust's message (a sentence). In the app
 * they go through Tauri's `invoke`; while Haunt is worked on in a browser, through the development bridge the dev
 * server forwards to (src-tauri/src/devbridge.rs). Every call about a repository names it first.
 */

export type News = ScanEvent & { repo: string | null }

interface Tauri {
  event: { listen: <T>(name: string, handler: (event: { payload: T }) => void) => Promise<() => void> }
}

function tauri(): Tauri | null {
  return import.meta.client && '__TAURI__' in window ? (window as unknown as { __TAURI__: Tauri }).__TAURI__ : null
}

// In a browser there is no Tauri: the bindings' `invoke` posts to the development bridge instead.
if (import.meta.client && !tauri()) {
  (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__ = {
    async invoke(command: string, args: object = {}) {
      const answer = await fetch(`/__haunt/call/${command}`, { method: 'POST', headers: { 'content-type': 'application/json', 'x-haunt': '1' }, body: JSON.stringify(args) })
      const body = await answer.json().catch(() => null) as unknown
      if (!answer.ok) throw (body as { message?: string } | null)?.message ?? `Haunt did not answer (${answer.status}).`
      return body
    }
  }
}

/** Every piece of news from Rust; returns a function that stops listening. */
export async function listen(handler: (news: News) => void): Promise<() => void> {
  const app = tauri()
  if (app) return app.event.listen<News>('haunt', event => handler(event.payload))
  // The development bridge's server-sent events, read with fetch (an EventSource cannot send the x-haunt header).
  const stop = new AbortController()
  void (async () => {
    while (!stop.signal.aborted) {
      try {
        const answer = await fetch('/__haunt/events', { headers: { 'x-haunt': '1' }, signal: stop.signal })
        const reader = answer.body!.pipeThrough(new TextDecoderStream()).getReader()
        let buffer = ''
        for (;;) {
          const { value, done } = await reader.read()
          if (done) break
          buffer += value
          let end: number
          while ((end = buffer.indexOf('\n\n')) >= 0) {
            const block = buffer.slice(0, end)
            buffer = buffer.slice(end + 2)
            const data = block.split('\n').filter(line => line.startsWith('data: ')).map(line => line.slice(6)).join('\n')
            if (data) handler(JSON.parse(data) as News)
          }
        }
      } catch { /* the app is starting or restarting: try again */ }
      if (!stop.signal.aborted) await new Promise(resolve => setTimeout(resolve, 1000))
    }
  })()
  return () => stop.abort()
}

/** The call graph, and how each TypeScript-family function is used: from another file, only its own, written into an object (held), or (absent) not at all. */
export interface Calls { scannedAt: string, calls: CallEdge[], uses?: Record<string, 'outside' | 'inside' | 'held'>, ms: number }

/** The commands, with the scan (the sidecar's, which Rust passes on as it is) typed from shared/types.ts. */
export const haunt = {
  ...commands,
  /** The latest structure (its `calls` are null; they come from `calls`), or null until the first scan is in. */
  structure: (repo: string) => commands.structure(repo) as Promise<Scan | null>,
  calls: (repo: string) => commands.calls(repo) as Promise<Calls | null>,
  editors: () => commands.editors() as Promise<Editor[]>
}
