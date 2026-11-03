<script lang="ts">
  import { t } from '../i18n.svelte';

  /** Shown while files are dragged over the window. */
  let { count }: { count: number } = $props();
</script>

<div class="ov" role="status" aria-live="polite">
  <div class="drop on">
    <img src="/bhworld-file.svg" alt="" width="72" height="72" />
    <h2>{t('drop.title')}</h2>
    <p class="muted">{t('drop.text', { n: count })}</p>
  </div>
</div>

<style>
  .ov { position: fixed; inset: var(--titlebar-h) 0 0 0; z-index: 60; background: var(--scrim); display: grid; padding: 32px; pointer-events: none; }
  .drop { background: var(--surface); }
  h2 { font-size: 26px; }
</style>
