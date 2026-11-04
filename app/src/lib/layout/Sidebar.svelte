<script lang="ts">
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import { store, type Screen } from '../store.svelte';
  import Icon from '../ui/Icon.svelte';

  const NAV: { id: Screen; icon: IconName }[] = [
    { id: 'devices', icon: 'devices' },
    { id: 'transfer', icon: 'swap' },
    { id: 'worlds', icon: 'library' },
    { id: 'backups', icon: 'shield' }
  ];
  const MORE: { id: Screen; icon: IconName }[] = [
    { id: 'activity', icon: 'history' },
    { id: 'settings', icon: 'sliders' }
  ];
  /** Read and write flows belong to Devices, the editor to Worlds. */
  const owner = (s: Screen): Screen => (s === 'read' || s === 'connect' ? 'devices' : s === 'editor' ? 'worlds' : s);
  const ready = $derived(store.devices.filter((d) => d.status === 'ready').length);
  const dev = $derived(store.connected);

  function go(id: Screen) {
    if (id === 'transfer') store.transfer.start();
    else store.go(id);
  }
</script>

{#snippet item(it: { id: Screen; icon: IconName })}
  <button class="nv" aria-current={owner(store.screen) === it.id ? 'page' : undefined} title={t(`nav.${it.id}` as 'nav.devices')} onclick={() => go(it.id)}>
    <Icon name={it.icon} />
    <span class="lbl">{t(`nav.${it.id}` as 'nav.devices')}</span>
    {#if it.id === 'devices' && ready > 0}<span class="badge" aria-label={t('nav.readyCount', { n: ready })}>{ready}</span>{/if}
  </button>
{/snippet}

<nav class={['wside', store.screen === 'editor' && 'mini']} aria-label={t('nav.aria')}>
  <div class="brand">
    <img src="/wt-icon.svg" alt="" width="36" height="36" />
    <span><small>Blockheads Club</small><b>World Tool</b></span>
  </div>
  {#each NAV as it (it.id)}{@render item(it)}{/each}
  <div class="nvsep" role="separator"></div>
  {#each MORE as it (it.id)}{@render item(it)}{/each}
  {#if dev}
    <div class="dcard" aria-label={t('side.connected')} role="group">
      <span class="hint">{t('side.connected')}</span>
      <span class="dn"><Icon name={dev.kind === 'pc' ? 'laptop' : 'phone'} size={18} />{dev.name}</span>
      <span class="ok"><span class="dot"></span>{dev.kind === 'pc' ? t('dev.ready') : t('side.readyVia', { via: t(`conn.${dev.connection}` as 'conn.usb') })}</span>
    </div>
  {/if}
</nav>

<style>
  .wside { width: 236px; flex: none; background: var(--surface); border-right: 2px solid var(--line); padding: 14px 12px; display: flex; flex-direction: column; gap: 4px; overflow: auto; }
  .brand { display: flex; align-items: center; gap: 10px; padding: 2px 8px 14px; }
  .brand img { flex: none; }
  small { display: block; font: 700 12px var(--f-body); color: var(--text-3); }
  b { display: block; font: 800 18px/1.1 var(--f-display); }
  .nv { display: flex; align-items: center; gap: 12px; min-height: 44px; padding: 6px 12px; border-radius: var(--r-md); color: var(--text-2); font: 700 16px/1.2 var(--f-body); background: transparent; border: 0; text-align: left; cursor: pointer; }
  .nv:hover { background: var(--surface-2); color: var(--text); }
  .nv[aria-current='page'] { background: var(--primary-soft); color: var(--primary-ink); }
  .lbl { flex: 1; min-width: 0; }
  .badge { margin-left: auto; padding: 2px 7px; border-radius: 999px; font: 700 11.5px/1.4 var(--f-body); background: var(--ok-soft); color: var(--ok); }
  .nvsep { height: 2px; background: var(--line); margin: 8px 6px; }
  .dcard { margin-top: auto; padding: 12px; border-radius: 12px; background: var(--surface-2); display: flex; flex-direction: column; gap: 6px; font-size: 13.5px; }
  .dcard .hint { font-weight: 700; }
  .dn { display: flex; gap: 8px; align-items: center; font-weight: 700; overflow-wrap: anywhere; }
  .ok { align-self: flex-start; display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border-radius: 999px; font: 700 12.5px/1.4 var(--f-body); background: var(--ok-soft); color: var(--ok); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
  /* The editor needs the width: icon rail, as in small windows. */
  .wside.mini { width: 76px; padding: 14px 10px; }
  .wside.mini :is(.lbl, .brand span, .dcard, .badge) { display: none; }
  .wside.mini .nv { justify-content: center; padding: 0; }
  .wside.mini .brand { justify-content: center; padding: 2px 0 14px; }
  /* Small windows (1024 × 640): icon rail. */
  @media (max-width: 1100px) {
    .wside { width: 76px; padding: 14px 10px; }
    .lbl, .brand span, .dcard, .badge { display: none; }
    .nv { justify-content: center; padding: 0; }
    .brand { justify-content: center; padding: 2px 0 14px; }
  }
</style>
