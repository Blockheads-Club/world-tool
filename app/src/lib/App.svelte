<script lang="ts">
  import { i18n, t } from './i18n.svelte';
  import Sidebar from './layout/Sidebar.svelte';
  import TitleBar from './layout/TitleBar.svelte';
  import Activity from './screens/Activity.svelte';
  import Backups from './screens/Backups.svelte';
  import Connect from './screens/Connect.svelte';
  import Devices from './screens/Devices.svelte';
  import DropOverlay from './screens/DropOverlay.svelte';
  import EditorPlaceholder from './screens/EditorPlaceholder.svelte';
  import ErrorDialog from './screens/ErrorDialog.svelte';
  import FirstRun from './screens/FirstRun.svelte';
  import GameFilesDialog from './screens/GameFilesDialog.svelte';
  import ImportDialog from './screens/ImportDialog.svelte';
  import ReadWorlds from './screens/ReadWorlds.svelte';
  import Settings from './screens/Settings.svelte';
  import Transfer from './screens/Transfer.svelte';
  import UpdateUi from './screens/UpdateUi.svelte';
  import Worlds from './screens/Worlds.svelte';
  import { store, type Screen } from './store.svelte';
  import Toasts from './ui/Toasts.svelte';

  const CTX: Partial<Record<Screen, () => string>> = {
    read: () => t('ctx.read'),
    transfer: () => t('nav.transfer'),
    worlds: () => t('nav.worlds'),
    backups: () => t('nav.backups'),
    activity: () => t('nav.activity'),
    settings: () => t('nav.settings'),
    editor: () => t('ctx.editor')
  };
  const ctx = $derived(store.settings?.first_run_done ? (CTX[store.screen]?.() ?? '') : '');
  const u = store.update;
  const chip = $derived.by(() => {
    if (!u.chip) return null;
    if (u.stage === 'downloading')
      return { label: t('upd.chip.downloading', { n: u.percent }), aria: t('upd.chip.downloadingAria', { n: u.percent }), icon: 'download' as const, tone: 'primary' as const, pct: u.percent, onclick: () => u.openChip() };
    if (u.stage === 'ready')
      return { label: t('upd.chip.restart'), aria: t('upd.chip.restartAria'), icon: 'refresh' as const, tone: 'ok' as const, pct: null, onclick: () => u.openChip() };
    if (u.stage === 'available')
      return { label: t('upd.chip.available', { v: u.info!.version }), aria: t('upd.chip.availableAria', { v: u.info!.version }), icon: 'upd' as const, tone: 'primary' as const, pct: null, onclick: () => u.openChip() };
    return { label: t('upd.chip.updates'), aria: t('upd.chip.updates'), icon: 'upd' as const, tone: 'plain' as const, pct: null, expanded: u.popover, onclick: () => u.openChip() };
  });
</script>

<div class="win" lang={i18n.lang}>
  <TitleBar os={store.info.os} {ctx} {chip} />
  {#if store.booted && store.settings}
    {#if !store.settings.first_run_done}
      <FirstRun />
    {:else}
      <div class="app">
        <Sidebar />
        <main class="content" id="main">
          {#if store.screen === 'devices'}<Devices />
          {:else if store.screen === 'connect'}<Connect />
          {:else if store.screen === 'read'}<ReadWorlds />
          {:else if store.screen === 'transfer'}<Transfer />
          {:else if store.screen === 'worlds'}<Worlds />
          {:else if store.screen === 'backups'}<Backups />
          {:else if store.screen === 'activity'}<Activity />
          {:else if store.screen === 'settings'}<Settings />
          {:else}<EditorPlaceholder />{/if}
        </main>
      </div>
      {#if store.importer.isOpen}<ImportDialog />{/if}
      {#if store.gameFiles.isOpen}<GameFilesDialog />{/if}
      {#if store.dragging !== null && !store.gameFiles.isOpen}<DropOverlay count={store.dragging} />{/if}
      <UpdateUi />
    {/if}
    {#if store.shown}<ErrorDialog shown={store.shown} onclose={() => (store.shown = null)} />{/if}
  {:else}
    <div class="app boot" role="status"><span class="spin"></span><span class="sr">{t('common.loading')}</span></div>
  {/if}
  <Toasts />
</div>

<style>
  .boot { display: grid; place-items: center; }
</style>
