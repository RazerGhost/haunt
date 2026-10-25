<script>
  import Counter from '$lib/Counter.svelte'
  import { legacy } from '../legacy.js'

  function load() {
    return legacy()
  }
</script>

<h1>Home</h1>
<Counter start={load()} />
