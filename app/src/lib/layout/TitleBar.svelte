<script lang="ts">
  import { onMount } from 'svelte';
  import { win } from '../api';
  import { t } from '../i18n.svelte';
  import Icon from '../ui/Icon.svelte';
  import type { IconName } from '../icons';

  type Os = 'windows' | 'macos' | 'linux';
  interface Props {
    os: Os;
    /** The part after "BH World Tool ·" (screen name); empty on first run. */
    ctx?: string;
    /** Update chip while the update popup is hidden (WT-Update); null: none. */
    chip?: { label: string; aria: string; icon: IconName; tone: 'primary' | 'ok' | 'plain'; pct: number | null; expanded?: boolean; onclick: () => void } | null;
  }
  let { os, ctx = '', chip = null }: Props = $props();

  let isMax = $state(false);
  let inactive = $state(false);

  onMount(() => {
    const sync = () => void win.isMaximized().then((m) => (isMax = m)).catch(() => {});
    sync();
    const offs: (() => void)[] = [];
    let gone = false;
    const keep = (p: Promise<() => void>) => void p.then((u) => (gone ? u() : offs.push(u))).catch(() => {});
    keep(win.onResized(sync));
    keep(win.onFocus((f) => (inactive = !f)));
    return () => {
      gone = true;
      offs.forEach((u) => u());
    };
  });

  const minimize = () => void win.minimize().catch(() => {});
  const toggle = () => void win.toggleMaximize().catch(() => {});
  const close = () => void win.close().catch(() => {});
  const maxLabel = $derived(isMax ? t('tb.restore') : t('tb.maximize'));
</script>

<header class={['bar', os, inactive && 'inactive']} aria-label={t('tb.label')} data-tauri-drag-region>
  {#if os === 'macos'}
    <div class="native" aria-hidden="true" data-tauri-drag-region></div>
  {/if}
  <div class="title" data-tauri-drag-region>
    <img src="/wt-icon-small.svg" alt="" width="16" height="16" data-tauri-drag-region />
    <span class="name" data-tauri-drag-region>BH World Tool</span>
    {#if ctx}<span class="ctx" data-tauri-drag-region>{`· ${ctx}`}</span>{/if}
  </div>
  {#if chip}
    <button class={['chip', chip.tone]} aria-haspopup="dialog" aria-expanded={chip.expanded} aria-label={chip.aria} onclick={chip.onclick}>
      <Icon name={chip.icon} size={14} />
      <span>{chip.label}</span>
      {#if chip.tone === 'primary' && chip.pct === null}<span class="ud" aria-hidden="true"></span>{/if}
      {#if chip.pct !== null}<span class="cb" style:width={`${chip.pct}%`}></span>{/if}
    </button>
  {/if}
  {#if os === 'windows'}
    <div class="wctl" role="group" aria-label={t('tb.controls')}>
      <button class="w" aria-label={t('tb.minimize')} onclick={minimize}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button class="w" aria-label={maxLabel} onclick={toggle}>
        {#if isMax}
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".6" y="2.6" width="6.8" height="6.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M2.6 2.6V1.6A1 1 0 0 1 3.6.6h4.8a1 1 0 0 1 1 1v4.8a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        {:else}
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".6" y=".6" width="8.8" height="8.8" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
        {/if}
      </button>
      <button class="w close" aria-label={t('tb.close')} onclick={close}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.8.8l8.4 8.4M9.2.8L.8 9.2" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  {:else if os === 'linux'}
    <div class="lctl" role="group" aria-label={t('tb.controls')}>
      <button class="l" aria-label={t('tb.minimize')} onclick={minimize}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 7h8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
      <button class="l" aria-label={maxLabel} onclick={toggle}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x="1.3" y="1.3" width="7.4" height="7.4" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
      </button>
      <button class="l close" aria-label={t('tb.close')} onclick={close}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 2l6 6M8 2L2 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      </button>
    </div>
  {/if}
</header>

<style>
  .bar {
    --b-bg: var(--surface-2);
    --b-line: var(--line);
    --b-fg: var(--text);
    --b-fg2: var(--text-3);
    --b-hov: rgba(31, 26, 23, 0.08);
    --b-round: var(--surface-3);
    --b-round-hov: #dcd2c6;
    box-sizing: border-box;
    flex: none;
    width: 100%;
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    background: var(--b-bg);
    border-bottom: 1px solid var(--b-line);
    color: var(--b-fg);
    user-select: none;
    -webkit-user-select: none;
  }
  :global(:root[data-theme='dark']) .bar { --b-bg: #1d1815; --b-line: #3a312b; --b-fg: #f5ede5; --b-fg2: #ae9f92; --b-hov: rgba(255,255,255,.12); --b-round: #312924; --b-round-hov: #4a413c; }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme='light']):not([data-theme='dark'])) .bar { --b-bg: #1d1815; --b-line: #3a312b; --b-fg: #f5ede5; --b-fg2: #ae9f92; --b-hov: rgba(255,255,255,.12); --b-round: #312924; --b-round-hov: #4a413c; }
  }
  .bar.inactive { --b-fg: var(--b-fg2); }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    height: 100%;
  }
  .macos .title { justify-content: center; padding-right: 78px; }
  .title img { flex: none; pointer-events: none; }
  .inactive .title img { opacity: 0.55; }
  .name { font: 700 13.5px/1 var(--f-body); white-space: nowrap; color: var(--b-fg); }
  .ctx { font: 400 13px/1 var(--f-body); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--b-fg2); }
  button { border: 0; margin: 0; padding: 0; display: grid; place-items: center; cursor: default; font: inherit; color: var(--b-fg); background: transparent; }
  button:focus-visible { outline: 3px solid var(--focus); outline-offset: -3px; }
  .wctl { display: flex; height: 100%; flex: none; }
  .w { width: 46px; height: 100%; }
  .w:hover { background: var(--b-hov); }
  .w.close:hover { background: #c8301e; color: #fff; }
  .lctl { display: flex; gap: 10px; align-items: center; padding: 0 10px; flex: none; }
  .l { width: 28px; height: 28px; border-radius: 50%; background: var(--b-round); }
  .l:hover { background: var(--b-round-hov); }
  .l.close:hover { background: #c8301e; color: #fff; }
  .chip { position: relative; overflow: hidden; display: inline-flex; gap: 6px; align-items: center; flex: none; height: 26px; padding: 0 10px; margin-right: 6px; border-radius: 999px; border: 1.5px solid var(--line-strong); background: var(--surface); font: 700 12.5px/1 var(--f-body); cursor: pointer; white-space: nowrap; }
  .chip:focus-visible { outline-offset: 1px; }
  .chip.primary { color: var(--primary-ink); background: var(--primary-soft); border-color: color-mix(in oklab, var(--primary), transparent 55%); }
  .chip.ok { color: var(--ok); background: var(--ok-soft); border-color: color-mix(in oklab, var(--ok), transparent 55%); }
  .chip.plain { color: var(--text-2); }
  .ud { width: 7px; height: 7px; border-radius: 50%; background: var(--primary); }
  .cb { position: absolute; left: 0; bottom: 0; height: 3px; background: var(--primary); }
  .native { width: 78px; height: 100%; flex: none; }
</style>
