<script lang="ts">
  import { blank, bytes, pct, zero } from '../format';
  import { t } from '../i18n.svelte';
  import type { Key } from '../messages';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import Progress from '../ui/Progress.svelte';

  /** "Get the phone tool": download Google's SDK Platform-Tools (adb) on first run; it is not shipped with the app. */
  const f = store.adb;
  const p = $derived(f.progress);
  const host = (url: string) => url.replace(/^https?:\/\//, '').split('/')[0];
  const title = $derived.by(() => {
    if (f.stage === 'done') return t('adb.doneTitle');
    if (f.stage === 'error') return t('adb.failTitle');
    if (f.stage === 'downloading') return t('adb.downloading');
    return t('adb.title');
  });
  const step = $derived(p?.stage === 'preparing' ? t('adb.unpacking') : p?.stage === 'checking' ? t('adb.checking') : t('adb.downloadingBar'));
</script>

<Dialog {title} icon={f.stage === 'done' ? 'check' : f.stage === 'error' ? 'alert' : 'download'} tone={f.stage === 'done' ? 'ok' : f.stage === 'error' ? 'warn' : 'p'}
  alert={f.stage === 'error'} busy={f.stage === 'downloading'} onclose={() => f.close()}>
  {#if f.stage === 'loading'}
    <p class="row" role="status"><span class="spin"></span>{t('adb.asking')}</p>
  {:else if f.stage === 'offer'}
    {@const i = f.info!}
    <p>{t('adb.text')}</p>
    <dl class="kv">
      <dt>{t('adb.from')}</dt><dd><b>Google</b> · <span class="mono url">{i.url}</span></dd>
      <dt>{t('adb.size')}</dt><dd>{i.size === null ? t('adb.sizeUnknown') : bytes(i.size)}</dd>
      <dt>{t('adb.to')}</dt><dd class="mono url">{i.folder}</dd>
    </dl>
    <p class="hint">{t('adb.terms', { host: host(i.terms_url) })} <span class="mono url">{i.terms_url}</span></p>
    <Notice icon="info" title={t('adb.otherTitle')}>{t('adb.otherText')}</Notice>
  {:else if f.stage === 'downloading'}
    <Progress title={step} value={p?.stage === 'downloading' && p.total ? pct(p.done, p.total) : null}
      detail={p?.stage === 'downloading' ? t('adb.detail', { done: bytes(zero(p.done)), total: p.total ? bytes(p.total) : '?' }) : ''} />
    <p class="hint">{t('adb.from')}: <span class="mono url">{blank(f.info?.url)}</span></p>
  {:else if f.stage === 'done'}
    <div class="res" role="status">
      <span class="ri"><Icon name="check" size={28} /></span>
      <div><b>{t('adb.doneText')}</b><p class="muted mono url">{blank(f.status?.path)}</p></div>
    </div>
  {:else}
    <p>{t(`err.${f.error!.code}` as Key)}</p>
    {#if f.error!.detail}<pre class="log"><span class="e">{f.error!.detail}</span></pre>{/if}
  {/if}
  {#snippet footer()}
    {#if f.stage === 'downloading'}
      <Button icon="x" onclick={() => f.close()}>{t('common.cancel')}</Button>
    {:else if f.stage === 'done'}
      <Button variant="primary" onclick={() => f.close()}>{t('common.done')}</Button>
    {:else}
      <Button variant="ghost" onclick={() => f.close()}>{t('common.notNow')}</Button>
      {#if f.stage === 'offer'}
        <Button variant="primary" icon="download" onclick={() => void f.download()}>{t('adb.download')}</Button>
      {:else if f.stage === 'error'}
        <Button variant="primary" icon="refresh" onclick={() => void (f.info ? f.download() : f.open())}>{t('common.retry')}</Button>
      {/if}
    {/if}
  {/snippet}
</Dialog>

<style>
  .row { display: flex; gap: 10px; align-items: center; }
  .url { overflow-wrap: anywhere; }
</style>
