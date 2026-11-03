<script lang="ts">
  import { bytes, day, pct } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';

  /** Update popup, "ready" notice, "restart after the transfer?" and the small result popover (WT-Update). */
  const u = store.update;
  let hideReady = $state(false);
  const pctRunning = $derived(pct(store.transfer.progress?.done, store.transfer.progress?.total));
</script>

{#if u.popup && u.info}
  <Dialog title={t('upd.title', { v: u.info.version })} icon="upd" onclose={() => u.later()}>
    <dl class="kv">
      <dt>{t('upd.new')}</dt><dd><b>{u.info.version}</b> · {day(u.info.date)} · {bytes(u.info.size)}</dd>
      <dt>{t('upd.have')}</dt><dd>{store.info.version}</dd>
    </dl>
    <b>{t('upd.whatsNew')}</b>
    <ul class="ol">{#each u.info.notes as n (n)}<li>{n}</li>{/each}</ul>
    <p class="hint">{t('upd.stays')}</p>
    {#snippet footer()}
      <button class="link skip" onclick={() => void u.skip()}>{t('upd.skip')}</button>
      <Button onclick={() => u.later()}>{t('common.later')}</Button>
      <Button variant="primary" icon="upd" onclick={() => void u.download()}>{t('upd.install')}</Button>
    {/snippet}
  </Dialog>
{/if}

{#if u.stage === 'ready' && !u.restartWhenDone && !hideReady}
  <div class="toastbox">
    <Notice tone="ok" icon="check" role="status" title={t('upd.readyTitle', { v: u.info!.version })}>
      {t('upd.readyText')}
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="refresh" onclick={() => void u.restart()}>{t('upd.restartNow')}</Button>
        <Button size="sm" variant="ghost" onclick={() => (hideReady = true)}>{t('common.later')}</Button>
      {/snippet}
    </Notice>
  </div>
{/if}

{#if u.waitDialog}
  <Dialog title={t('upd.waitTitle')} icon="alert" tone="warn" onclose={() => (u.waitDialog = false)}>
    <p>{t('upd.waitText', { name: store.transfer.plan?.target ?? store.read.device?.name ?? '', n: pctRunning })}</p>
    <Notice icon="refresh" title={t('upd.waitNoticeTitle')}>{t('upd.waitNoticeText')}</Notice>
    {#snippet footer()}
      <Button onclick={() => (u.waitDialog = false)}>{t('common.notNow')}</Button>
      <Button variant="primary" icon="refresh" onclick={() => u.waitForTransfer()}>{t('upd.whenDone')}</Button>
    {/snippet}
  </Dialog>
{/if}

{#if u.popover && (u.stage === 'latest' || u.stage === 'failed')}
  <div class="card pop" role="dialog" aria-label={t('upd.chip.updates')}>
    {#if u.stage === 'latest'}
      <span class="ok"><Icon name="check" /></span>
      <b>{t('upd.latest')}</b>
      <p class="muted">{t('upd.latestText', { v: store.info.version })}</p>
    {:else}
      <span class="bad"><Icon name="wifioff" /></span>
      <b>{t('upd.failed')}</b>
      <p class="muted">{t('upd.failedText')}</p>
    {/if}
    <div class="kpi">
      <Button size="sm" icon="refresh" onclick={() => void u.check()}>{u.stage === 'latest' ? t('upd.again') : t('common.retry')}</Button>
      {#if u.stage === 'latest'}<Button size="sm" variant="ghost" onclick={() => { u.popover = false; store.go('settings'); }}>{t('upd.settings')}</Button>{/if}
      <Button size="sm" variant="ghost" icon="x" label={t('common.close')} onclick={() => (u.popover = false)} />
    </div>
  </div>
{/if}

<style>
  .skip { margin-right: auto; }
  .toastbox { position: fixed; right: 24px; bottom: 24px; z-index: 40; width: min(420px, calc(100vw - 48px)); box-shadow: var(--lift); border-radius: 14px; background: var(--surface); }
  .pop { position: fixed; right: 150px; top: calc(var(--titlebar-h) + 4px); z-index: 45; width: 340px; padding: 16px; display: flex; flex-direction: column; gap: 10px; box-shadow: var(--lift); }
  .ok { color: var(--ok); }
  .bad { color: var(--bad); }
</style>
