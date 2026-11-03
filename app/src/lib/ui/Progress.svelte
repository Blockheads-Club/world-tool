<script lang="ts">
  import type { Snippet } from 'svelte';

  /** TransferProgress: determinate (`value` 0–100) or busy bar with title, percent, detail and actions. */
  interface Props {
    title: string;
    value?: number | null;
    detail?: string;
    actions?: Snippet;
  }
  let { title, value = null, detail, actions }: Props = $props();
</script>

<div class="tp">
  <div class="top"><b>{title}</b>{#if value !== null}<span class="mono">{`${value}%`}</span>{/if}</div>
  <div
    class={['bar', value === null && 'busy']}
    role="progressbar"
    aria-label={title}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={value ?? undefined}
  >
    <span style:width={value === null ? undefined : `${value}%`}></span>
  </div>
  {#if detail || actions}
    <div class="bot"><span class="hint" aria-live="polite">{detail}</span>{@render actions?.()}</div>
  {/if}
</div>

<style>
  .tp { display: flex; flex-direction: column; gap: 10px; }
  .top, .bot { display: flex; justify-content: space-between; gap: 12px; align-items: center; flex-wrap: wrap; }
  .top .mono { font-weight: 600; }
</style>
