/*
 * One dialog for the app's questions (AppDialog shows it, once, in app.vue): a name or a message to type, with a
 * checkbox; a yes or no; or a choice between a few answers. Each call waits for the answer: null when cancelled.
 */

export interface Choice<T> { label: string, value: T, color?: 'primary' | 'neutral' | 'error' }

export interface Asking {
  title: string
  description?: string
  /** A text to type: a name (one line) or a message (several). */
  input?: { label: string, value: string, placeholder?: string, multiline?: boolean, required?: boolean, secret?: boolean }
  /** A second text, optional. */
  note?: { label: string, value: string, placeholder?: string }
  checkbox?: { label: string, value: boolean }
  /** The answers as buttons, the first being the main one; by default OK and Cancel. */
  choices: Choice<string>[]
  done: (answer: { choice: string, input: string, note: string, checked: boolean } | null) => void
}

const asking = shallowRef<Asking | null>(null)

function ask(question: Omit<Asking, 'done'>): Promise<{ choice: string, input: string, note: string, checked: boolean } | null> {
  // A question still open is cancelled by the new one.
  asking.value?.done(null)
  return new Promise((done) => {
    asking.value = {
      ...question,
      done: (answer) => {
        asking.value = null
        done(answer)
      }
    }
  })
}

export function useDialog() {
  return {
    asking,
    /** A text to type (with an optional note and checkbox); null when cancelled. */
    async prompt(options: { title: string, description?: string, input: Asking['input'], note?: Asking['note'], checkbox?: Asking['checkbox'], confirm: string }) {
      const answer = await ask({ ...options, choices: [{ label: options.confirm, value: 'ok', color: 'primary' }] })
      // A secret is passed on exactly as typed.
      return answer && { text: options.input?.secret ? answer.input : answer.input.trim(), note: answer.note.trim(), checked: answer.checked }
    },
    /** Yes or no; `danger` makes the yes red. */
    async confirm(options: { title: string, description: string, confirm: string, danger?: boolean }): Promise<boolean> {
      const answer = await ask({ title: options.title, description: options.description, choices: [{ label: options.confirm, value: 'ok', color: options.danger ? 'error' : 'primary' }] })
      return answer?.choice === 'ok'
    },
    /** One of a few answers; null when cancelled. */
    async choose<T extends string>(options: { title: string, description: string, choices: Choice<T>[] }): Promise<T | null> {
      const answer = await ask(options)
      return (answer?.choice as T | undefined) ?? null
    }
  }
}
