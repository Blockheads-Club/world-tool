<script lang="ts">
  import { onMount } from 'svelte';
  import { inTauri, tauriBackend } from '$lib/api';
  import App from '$lib/App.svelte';
  import { mockFromUrl } from '$lib/mock-api';
  import { store } from '$lib/store.svelte';

  // Outside Tauri (bun run dev, e2e) the in-memory mock backend stands in; `?scenario=empty` etc. pick its data.
  onMount(() => void store.init(inTauri() ? tauriBackend : mockFromUrl(location.search)));
</script>

<App />
