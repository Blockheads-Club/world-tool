<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  /** EmptyState: three blocks (or an icon), title, text, actions. */
  let { title, text, icon, children }: { title: string; text?: string; icon?: IconName; children?: Snippet } = $props();
</script>

<div class="card empty-card">
  {#if icon}
    <span class="ic"><Icon name={icon} size={32} /></span>
  {:else}
    <div class="blk" aria-hidden="true"><span style:background="#5FB548"></span><span style:background="#8A5A36"></span><span style:background="#7A7068"></span></div>
  {/if}
  <h2>{title}</h2>
  {#if text}<p class="muted">{text}</p>{/if}
  {#if children}<div class="kpi act">{@render children()}</div>{/if}
</div>

<style>
  h2 { font-size: 24px; }
  p { max-width: 560px; }
  .blk { display: flex; gap: 6px; margin-bottom: 6px; }
  .blk span { width: 44px; height: 44px; border-radius: 10px; box-shadow: 0 4px 0 rgba(0, 0, 0, 0.18); }
  .ic { width: 64px; height: 64px; border-radius: 18px; display: grid; place-items: center; background: var(--surface-2); color: var(--text-2); }
  .act { justify-content: center; }
</style>
