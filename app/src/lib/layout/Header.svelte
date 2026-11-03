<script lang="ts">
  import type { Snippet } from 'svelte';

  /** Screen header: title, subtitle, actions on the right. */
  let { title, sub, actions }: { title: string; sub?: string; actions?: Snippet } = $props();
</script>

<header class="head">
  <div class="tt">
    <h1>{title}</h1>
    {#if sub}<p class="muted">{sub}</p>{/if}
  </div>
  {#if actions}<div class="acts">{@render actions()}</div>{/if}
</header>

<style>
  .head { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 16px 28px; border-bottom: 2px solid var(--line); flex-wrap: wrap; flex: none; }
  .tt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  h1 { font-size: 26px; }
  .muted { font-size: 14.5px; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
  @media (max-width: 1100px) { .head { padding: 14px 20px; } }
</style>
