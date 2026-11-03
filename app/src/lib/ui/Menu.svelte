<script lang="ts">
  import { tick } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  /** "More actions" button with a small menu (arrow keys move, Esc closes and returns focus). */
  interface Item {
    label: string;
    icon: IconName;
    danger?: boolean;
    run: () => void;
  }
  let { label, items }: { label: string; items: Item[] } = $props();
  let open = $state(false);
  let btn: HTMLButtonElement | undefined = $state();
  let list: HTMLElement | undefined = $state();

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      list?.querySelector<HTMLElement>('[role=menuitem]')?.focus();
    }
  }

  function close() {
    open = false;
    btn?.focus();
  }

  function key(e: KeyboardEvent) {
    const els = [...(list as HTMLElement).querySelectorAll<HTMLElement>('[role=menuitem]')];
    const i = els.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      els[(i + (e.key === 'ArrowDown' ? 1 : -1) + els.length) % els.length].focus();
    }
  }
</script>

<span class="mw">
  <button bind:this={btn} class="mb" aria-label={label} aria-haspopup="menu" aria-expanded={open} onclick={toggle}><Icon name="more" /></button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="shield" onmousedown={close}></div>
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="menu" role="menu" aria-label={label} bind:this={list} onkeydown={key}>
      {#each items as it (it.label)}
        <button role="menuitem" class:danger={it.danger} onclick={() => { open = false; it.run(); }}>
          <Icon name={it.icon} size={18} />{it.label}
        </button>
      {/each}
    </div>
  {/if}
</span>

<style>
  .mw { position: relative; display: inline-flex; }
  .mb { width: 40px; min-height: 40px; border-radius: var(--r-md); border: 2px solid transparent; background: transparent; color: var(--text-2); display: grid; place-items: center; cursor: pointer; }
  .mb:hover { background: var(--surface-2); color: var(--text); }
  .shield { position: fixed; inset: 0; z-index: 30; }
  .menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 31; min-width: 220px; padding: 6px; background: var(--surface); border: 2px solid var(--line-strong); border-radius: 12px; box-shadow: var(--lift); display: flex; flex-direction: column; }
  .menu button { display: flex; align-items: center; gap: 10px; min-height: 40px; padding: 0 12px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font: 700 14px var(--f-body); text-align: left; cursor: pointer; white-space: nowrap; }
  .menu button:hover, .menu button:focus-visible { background: var(--surface-2); }
  .menu button.danger { color: var(--bad); }
</style>
