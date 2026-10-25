<script lang="ts">
  import { format } from './format'

  export let start = 0

  function increment(): string {
    return format(start + 1)
  }
</script>

<button on:click={increment}>{start}</button>
