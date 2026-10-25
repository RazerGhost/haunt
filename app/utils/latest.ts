import type { Ref } from 'vue'

/*
 * "Only the latest answer counts", for anything that asks Rust again while an older question may still be on its way:
 * a new question, a new repository (another tab) or `cancel()` makes older answers land nowhere. `run` gives the answer,
 * or undefined when it no longer counts; an error that no longer counts is dropped too. `loading` is true while the
 * latest question is on its way: an older answer landing late does not end it.
 */
export function latest(repo: Ref<string | null>) {
  let asked = 0
  const loading = ref(false)
  return {
    loading,
    async run<T>(load: (repo: string) => Promise<T>): Promise<T | undefined> {
      const ask = ++asked
      const at = repo.value
      if (!at) {
        loading.value = false
        return undefined
      }
      loading.value = true
      const counts = () => ask === asked && repo.value === at
      try {
        const value = await load(at)
        return counts() ? value : undefined
      } catch (error) {
        if (counts()) throw error
        return undefined
      } finally {
        if (ask === asked) loading.value = false
      }
    },
    /** Whatever is on its way now no longer counts. */
    cancel(): void {
      asked++
      loading.value = false
    }
  }
}
