<script setup lang="ts">
import type { Health } from '~/bindings'

/*
 * Repository health (src-tauri/src/git/health.rs): how much room the history takes, the files that take the most of it
 * (deleted ones too: the history keeps every version), the files that belong in Git LFS, and branches nobody has
 * touched in a long time. Read only: it says what could be done, and leaves the rewriting tools to you.
 */
const { repo } = useRepositories()
const { refs } = useRefs()
const failure = useFailure()

const health = shallowRef<Health | null>(null)
// A slow answer for the tab before lands nowhere.
const ask = latest(repo)
const { loading } = ask
async function load(): Promise<void> {
  health.value = null
  if (!repo.value) return
  try {
    const found = await ask.run(at => haunt.health(at))
    if (found !== undefined) health.value = found
  } catch (error) {
    failure.failed('The health could not be read', error)
  }
}
watch(repo, () => void load(), { immediate: true })

/** Branches whose last commit is older than this are old. */
const OLD_DAYS = 90
const old = computed(() => {
  const cutoff = Date.now() / 1000 - OLD_DAYS * 86_400
  return (refs.value?.branches ?? []).filter(branch => !branch.current && branch.time < cutoff).sort((a, b) => a.time - b.time)
})
const cleaning = ref(false)
const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString()
const deletedBytes = computed(() => (health.value?.largest ?? []).filter(file => !file.inHead).reduce((sum, file) => sum + file.bytes, 0))
</script>

<template>
  <div class="mx-auto grid w-full max-w-3xl content-start gap-6 px-4 py-8">
    <div class="flex items-start gap-3">
      <div class="grid flex-1 gap-1">
        <h1 class="text-lg font-semibold text-highlighted">Health</h1>
        <p class="text-sm text-muted">History size, heavy files, Git LFS candidates and old branches. Read-only.</p>
      </div>
      <UButton label="Read again" icon="i-lucide-refresh-cw" size="sm" color="neutral" variant="ghost" :loading="loading" @click="load" />
    </div>
    <p v-if="loading && !health" class="text-sm text-muted">Going through every object in the history…</p>

    <template v-if="health">
      <section class="grid gap-1">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">Size</h2>
        <p class="text-2xl font-semibold text-highlighted tabular-nums">{{ formatBytes(health.bytes) }}</p>
        <p class="text-sm text-muted">The whole history on disk, in {{ health.packs }} {{ health.packs === 1 ? 'pack' : 'packs' }}<template v-if="health.looseObjects">, with {{ health.looseObjects }} loose {{ health.looseObjects === 1 ? 'object' : 'objects' }} git packs at its next cleanup (<code>git gc</code>)</template>. Every clone downloads it.</p>
      </section>

      <section class="grid gap-2">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">Largest files in the history</h2>
        <p class="text-sm text-muted">Every version counts: a file changed often or deleted long ago still weighs on every clone.<template v-if="deletedBytes"> {{ formatBytes(deletedBytes) }} of these are files the last commit no longer has; only rewriting the history (with <code>git filter-repo</code>) takes them out, and everyone then clones again.</template></p>
        <ul class="grid gap-0.5">
          <li v-for="file in health.largest" :key="file.path" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
            <span class="min-w-0 flex-1 truncate font-mono text-xs" :class="{ 'text-muted line-through': !file.inHead }" :title="file.path">{{ file.path }}</span>
            <UBadge v-if="!file.inHead" label="deleted" size="sm" color="neutral" variant="subtle" />
            <span class="text-xs text-muted tabular-nums">{{ file.versions }} {{ file.versions === 1 ? 'version' : 'versions' }}</span>
            <span class="w-20 text-right font-medium tabular-nums">{{ formatBytes(file.bytes) }}</span>
          </li>
        </ul>
      </section>

      <section class="grid gap-2">
        <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">For Git LFS</h2>
        <p v-if="!health.lfs.length" class="text-sm text-muted">{{ health.usesLfs ? 'The large files are in Git LFS already.' : 'No file is big enough to need Git LFS.' }}</p>
        <template v-else>
          <p class="text-sm text-muted">Large or binary files over 256 KB are stored whole in every version; Git LFS stores pointers instead. To move them: <code>git lfs track</code> with their pattern, then commit (and <code>git lfs migrate</code> for the versions already in the history).</p>
          <ul class="grid gap-0.5">
            <li v-for="file in health.lfs" :key="file.path" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
              <span class="min-w-0 flex-1 truncate font-mono text-xs" :title="file.path">{{ file.path }}</span>
              <span class="w-20 text-right tabular-nums">{{ formatBytes(file.bytes) }}</span>
            </li>
          </ul>
        </template>
      </section>
    </template>

    <section class="grid gap-2">
      <h2 class="text-xs font-semibold tracking-wider text-muted uppercase">Old branches</h2>
      <p v-if="!old.length" class="text-sm text-muted">Every branch had a commit in the last {{ OLD_DAYS }} days.</p>
      <template v-else>
        <p class="text-sm text-muted">{{ old.length }} {{ old.length === 1 ? 'branch has' : 'branches have' }} had no commit in {{ OLD_DAYS }} days. Clean up branches shows which are merged and safe to delete.</p>
        <ul class="grid gap-0.5">
          <li v-for="branch in old" :key="branch.name" class="flex items-center gap-3 rounded px-2 py-1 text-sm hover:bg-elevated">
            <span class="min-w-0 flex-1 truncate">{{ branch.name }}</span>
            <span class="min-w-0 max-w-64 truncate text-xs text-muted">{{ branch.subject }}</span>
            <span class="text-xs text-muted tabular-nums">{{ day(branch.time) }}</span>
          </li>
        </ul>
        <UButton label="Clean up branches…" icon="i-lucide-brush-cleaning" size="sm" color="neutral" variant="outline" class="justify-self-start" @click="cleaning = true" />
      </template>
    </section>
    <BranchCleanup v-model:open="cleaning" />
  </div>
</template>
