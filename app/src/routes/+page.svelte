<script lang="ts">
  import { onMount } from 'svelte';
  import App from '$lib/App.svelte';
  import { chooseBackend } from '$lib/mock-api';
  import { store } from '$lib/store.svelte';

  // Outside Tauri (bun run dev, e2e) the in-memory mock backend stands in; `?scenario=empty` etc. pick its data.
  onMount(() => void store.init(chooseBackend(location.search)));
</script>

<App />
