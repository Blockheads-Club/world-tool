<script lang="ts" generics="V extends string">
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  interface Props {
    label: string;
    value: V;
    options: { value: V; label: string; icon?: IconName }[];
    onchange: (v: V) => void;
    size?: 'md' | 'sm';
  }
  let { label, value, options, onchange, size = 'md' }: Props = $props();
  let group: HTMLElement | undefined = $state();

  /** Radio-group keyboard model: arrows move the choice, only the chosen option is in the tab order. */
  function key(e: KeyboardEvent) {
    const i = options.findIndex((o) => o.value === value);
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = (i + step + options.length) % options.length;
    onchange(options[next].value);
    group?.querySelectorAll<HTMLElement>('[role=radio]')[next]?.focus();
  }
</script>

<div class={['seg', size]} role="radiogroup" aria-label={label} bind:this={group}>
  {#each options as o (o.value)}
    <button type="button" role="radio" aria-checked={o.value === value} tabindex={o.value === value ? 0 : -1} onclick={() => onchange(o.value)} onkeydown={key}>
      {#if o.icon}<Icon name={o.icon} size={18} />{/if}{o.label}
    </button>
  {/each}
</div>

<style>
  .seg { display: inline-flex; flex-wrap: wrap; border: 2px solid var(--line-strong); border-radius: var(--r-md); overflow: hidden; flex: none; max-width: 100%; }
  button { min-height: 40px; padding: 0 14px; border: 0; background: var(--surface); color: var(--text-2); font: 700 14px var(--f-body); cursor: pointer; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .sm button { min-height: 36px; padding: 0 12px; font-size: 13.5px; }
  button + button { border-left: 2px solid var(--line-strong); }
  button:hover { color: var(--text); }
  button[aria-checked='true'] { background: var(--text); color: var(--bg); }
  button:focus-visible { outline-offset: -4px; }
</style>
