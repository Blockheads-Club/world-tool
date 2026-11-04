<script lang="ts">
  import { t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { Device } from '../types';
  import Button from '../ui/Button.svelte';
  import Empty from '../ui/Empty.svelte';
  import Notice from '../ui/Notice.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';
  import DeviceRow from './DeviceRow.svelte';

  const phones = $derived(store.devices.filter((d) => d.kind === 'android'));
  const local = $derived(store.devices.filter((d) => d.kind === 'pc'));
  const ios = $derived(store.devices.filter((d) => d.kind === 'ios'));

  async function forget(d: Device) {
    await store.api.forgetPaired(d.id);
    await store.refreshDevices();
  }

  async function diag() {
    toast((await copyText(await store.api.diagnostics())) ? t('common.copied') : t('common.copyFailed'));
  }

  const groups = $derived([
    { id: 'phones', title: t('dev.phones'), list: phones },
    { id: 'local', title: t('dev.local'), list: local },
    { id: 'ios', title: t('dev.ios'), list: ios }
  ].filter((g) => g.list.length > 0));
</script>

<Header title={t('dev.title')} sub={t('dev.sub')}>
  {#snippet actions()}
    <Button icon="refresh" loading={store.scanning} onclick={() => void store.refreshDevices()}>{t('common.refresh')}</Button>
    <Button variant="primary" icon="plus" onclick={() => store.openGuide('usb')}>{t('dev.add')}</Button>
  {/snippet}
</Header>
<div class="main">
  {#if store.adb.missing}
    <Notice tone="warn" icon="download" title={t('adb.missingTitle')}>
      {t('adb.missingText')}
      {#snippet actions()}<Button size="sm" variant="primary" icon="download" onclick={() => void store.adb.open()}>{t('adb.get')}</Button>{/snippet}
    </Notice>
  {/if}
  {#if store.devices.length === 0 && store.scanning}
    <div class="card empty-card" role="status">
      <span class="spin big-spin"></span>
      <h2>{t('dev.scan')}</h2>
      <p class="muted">{t('dev.scanSub')}</p>
    </div>
  {:else if store.devices.length === 0}
    <Empty title={t('dev.noneTitle')} text={t('dev.noneText')}>
      <Button icon="usb" onclick={() => store.openGuide('usb')}>{t('guide.usb')}</Button>
      <Button icon="wifi" onclick={() => store.openGuide('wifi')}>{t('guide.wifi')}</Button>
      <Button icon="archive" onclick={() => store.openGuide('ios')}>{t('guide.ios')}</Button>
    </Empty>
    <button class="link center" onclick={() => store.openGuide('trouble')}>{t('dev.trouble')}</button>
  {:else}
    {#each groups as g (g.id)}
      <section class="card" aria-label={g.title}>
        <h2 class="gh">{g.title}</h2>
        {#each g.list as d (d.id)}
          <DeviceRow device={d} onforget={() => void forget(d)} ondiag={() => void diag()} />
        {/each}
      </section>
    {/each}
  {/if}
</div>

<style>
  .gh { font-size: 16px; padding: 14px 18px 0; color: var(--text-2); }
  .big-spin { width: 40px; height: 40px; border-width: 4px; }
  .empty-card h2 { font-size: 22px; }
  .center { align-self: center; }
</style>
