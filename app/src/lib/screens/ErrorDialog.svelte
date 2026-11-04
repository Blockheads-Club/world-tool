<script lang="ts">
  import { bytes, dash } from '../format';
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import type { Key } from '../messages';
  import { store, type Guide, type ShownError } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Notice from '../ui/Notice.svelte';
  import Phone from '../ui/Phone.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';

  /** The designed error dialogs (WT-Errors). */
  let { shown, onclose }: { shown: ShownError; onclose: () => void } = $props();
  const e = $derived(shown.error);
  const dev = $derived(e.device ?? t('err.thePhone'));
  const ICON: Record<string, IconName> = {
    not_allowed: 'lock', game_not_installed: 'box', unsupported_version: 'alert', declined: 'x', wrong_password: 'lock',
    backup_empty: 'alert', disconnected: 'unplug', disk_full: 'disk', world_damaged: 'file', restore_declined: 'phone'
  };
  const title = $derived(t(`err.${e.code}.title` as Key, { device: dev, name: e.name ?? '' }));
  let way = $state<Guide | 'devices'>('wifi');

  function retry() {
    const again = shown.retry;
    onclose();
    again?.();
  }

  function next() {
    onclose();
    if (way === 'devices') store.go('devices');
    else store.openGuide(way);
  }

  function restore() {
    const id = e.backup_id as string;
    onclose();
    store.restoreId = id;
    store.go('backups');
  }

  async function details() {
    toast((await copyText(e.detail ?? e.code)) ? t('common.copied') : t('common.copyFailed'));
  }
</script>

<Dialog {title} icon={ICON[e.code]} tone={e.code === 'disconnected' || e.code === 'world_damaged' ? 'bad' : 'warn'} alert={e.code === 'disconnected' || e.code === 'world_damaged'} {onclose}>
  {#if e.code === 'not_allowed'}
    <div class="row">
      <Phone screen="allow" />
      <div class="col">
        <p>{t('err.not_allowed.text', { device: dev })}</p>
        <ol class="ol"><li>{t('err.not_allowed.s1')}</li><li>{t('err.not_allowed.s2')}</li><li>{t('err.not_allowed.s3')}</li></ol>
        <p class="hint">{t('err.not_allowed.hint')}</p>
      </div>
    </div>
  {:else if e.code === 'game_not_installed'}
    <p>{t('err.game_not_installed.text')}</p>
    <p class="hint">{t('err.game_not_installed.hint')}</p>
  {:else if e.code === 'unsupported_version'}
    <dl class="kv"><dt>{t('err.onDevice', { device: dev })}</dt><dd>The Blockheads {dash(e.version)}</dd><dt>{t('err.supported')}</dt><dd>1.7.4 – 1.7.6</dd></dl>
    <p>{t('err.unsupported_version.text')}</p>
  {:else if e.code === 'declined'}
    <p>{t('err.declined.text', { device: dev })}</p>
  {:else if e.code === 'wrong_password'}
    <p>{t('err.wrong_password.text')}</p>
    <div class="ffield">
      <label for="pw2">{t('err.wrong_password.label')}</label>
      <input id="pw2" class="input" type="password" bind:value={store.read.password} aria-invalid="true" aria-describedby="pw2h" data-autofocus />
      <span class="hint" id="pw2h">{t('err.wrong_password.hint', { n: (e.attempt ?? 1) + 1 })}</span>
    </div>
  {:else if e.code === 'backup_empty'}
    <p>{t('err.backup_empty.text')}</p>
    <div class="radios" role="radiogroup" aria-label={t('err.backup_empty.ways')}>
      {#each [{ v: 'wifi', ic: 'wifi' }, { v: 'root', ic: 'root' }, { v: 'devices', ic: 'devices' }] as o (o.v)}
        <label class={['radio', way === o.v && 'on']}>
          <input type="radio" name="way" checked={way === o.v} onchange={() => (way = o.v as Guide | 'devices')} />
          <span class="rd" aria-hidden="true"></span>
          <span><b>{t(`err.backup_empty.${o.v}` as Key)}</b><br /><span class="hint">{t(`err.backup_empty.${o.v}Hint` as Key)}</span></span>
        </label>
      {/each}
    </div>
  {:else if e.code === 'disconnected'}
    <p>{t('err.disconnected.text', { device: dev, n: e.percent ?? 0 })}</p>
    {#if e.backup_id}<Notice icon="shield" title={t('err.disconnected.safe')}>{t('err.disconnected.safeText')}</Notice>{/if}
  {:else if e.code === 'restore_declined'}
    <div class="row">
      <Phone screen="restore" />
      <div class="col">
        <p>{t('err.restore_declined.text', { device: dev })}</p>
        <p class="hint">{t('err.restore_declined.hint')}</p>
        {#if e.backup_id}<Notice icon="shield" title={t('err.disconnected.safe')}>{t('err.restore_declined.safe')}</Notice>{/if}
      </div>
    </div>
  {:else if e.code === 'disk_full'}
    <dl class="kv"><dt>{t('err.needed')}</dt><dd>{t('tr.about', { size: bytes(e.needed ?? 0) })}</dd><dt>{t('err.freeOn', { disk: e.disk ?? '' })}</dt><dd>{bytes(e.free ?? 0)}</dd></dl>
    <p>{t('err.disk_full.text')}</p>
  {:else}
    <p>{t('err.world_damaged.text')}</p>
    {#if e.detail}<pre class="log"><span class="e">{e.detail}</span></pre>{/if}
  {/if}
  {#snippet footer()}
    {#if e.code === 'backup_empty' || e.code === 'world_damaged'}<Button variant="ghost" icon="copy" onclick={() => void details()}>{t('common.copyDetails')}</Button>{/if}
    {#if e.code === 'disconnected'}
      <Button onclick={onclose}>{t('common.later')}</Button>
      {#if e.backup_id}<Button variant="primary" icon="history" onclick={restore}>{t('tr.restoreBackup')}</Button>{/if}
    {:else if e.code === 'backup_empty'}
      <Button onclick={onclose}>{t('common.cancel')}</Button>
      <Button variant="primary" icon="chevr" onclick={next}>{t('common.continue')}</Button>
    {:else if e.code === 'world_damaged'}
      <Button onclick={onclose}>{t('common.close')}</Button>
      {#if shown.retry}<Button variant="danger" icon="trash" onclick={retry}>{t('err.world_damaged.remove')}</Button>{/if}
    {:else}
      <Button onclick={onclose}>{e.code === 'declined' || e.code === 'disk_full' || e.code === 'wrong_password' ? t('common.cancel') : t('common.close')}</Button>
      {#if shown.retry}
        <Button variant="primary" icon={e.code === 'disk_full' ? 'folder' : 'refresh'} onclick={retry}>
          {e.code === 'disk_full' ? t('err.disk_full.other') : e.code === 'declined' ? t('err.startAgain') : t('common.retry')}
        </Button>
      {/if}
    {/if}
  {/snippet}
</Dialog>

<style>
  .row { display: flex; gap: 24px; align-items: center; flex-wrap: wrap; }
  .col { flex: 1 1 260px; display: flex; flex-direction: column; gap: 10px; }
  .radios { display: flex; flex-direction: column; gap: 10px; }
  .radio { display: flex; gap: 12px; align-items: flex-start; padding: 12px 14px; border: 2px solid var(--line); border-radius: 14px; cursor: pointer; position: relative; }
  .radio.on { border-color: var(--primary); background: var(--primary-soft); }
  .radio input { position: absolute; opacity: 0; }
  .radio:has(input:focus-visible) { outline: 3px solid var(--focus); outline-offset: 2px; }
  .rd { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--line-strong); flex: none; background: var(--surface); margin-top: 2px; }
  .radio.on .rd { border: 7px solid var(--primary); }
</style>
