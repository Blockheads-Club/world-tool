<script lang="ts">
  import { bytes, eta, pct } from '../format';
  import { t } from '../i18n.svelte';
  import type { Key } from '../messages';
  import { store } from '../store.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import Progress from '../ui/Progress.svelte';
  import { toast } from '../ui/toast.svelte';
  import { baseName, copyText, dirName } from '../util';

  /** GameFilesStatus dialog: the user's APK → extracted graphics, before the editor (WT-GameFiles). */
  const f = store.gameFiles;
  const p = $derived(f.progress);
  const phone = $derived(store.readyPhones[0] ?? null);
  const code = $derived(f.error?.code);
  const title = $derived.by(() => {
    if (f.stage === 'checking') return t('gf.checking');
    if (f.stage === 'extracting') return t('gf.extracting');
    if (f.stage === 'ready') return t('gf.ready');
    if (code === 'apk_wrong_app') return t('gf.wrongTitle');
    if (code === 'apk_unsupported') return t('err.unsupported_version.title');
    if (code === 'apk_damaged') return t('gf.damagedTitle');
    if (f.stage === 'error') return t('gf.failTitle');
    return f.missing ? t('gf.missingTitle') : t('gf.askTitle');
  });
  const ok = $derived(f.stage === 'ready');
  const errTone = $derived(code === 'apk_wrong_app' || code === 'apk_damaged' ? 'bad' : 'warn');
  // Our own stand-in colours for blocks (the real graphics come from the user's APK).
  const SWATCH = ['#5FB548', '#8A5A36', '#7A7068', '#E9C46A', '#F2F6FA', '#3A7BD5', '#B5651D', '#9AD6F7', '#4E944F', '#C2410C', '#6E625A', '#FFD27A'];
  const checks: Key[] = ['gf.c1', 'gf.c2', 'gf.c3'];
  async function details() {
    toast((await copyText(f.error?.detail ?? f.error?.code ?? '')) ? t('common.copied') : t('common.copyFailed'));
  }
</script>

{#snippet dropzone()}
  <div class={['drop', store.dragging !== null && 'on']}>
    <Icon name="upload" size={28} />
    <b>{store.dragging !== null ? t('gf.dropNow') : t('gf.drop')}</b>
    <span class="hint"><Button size="sm" icon="folder" onclick={() => void f.choose()}>{t('gf.choose')}</Button> {t('gf.versions')}</span>
  </div>
{/snippet}

{#snippet file(name: string, line: string)}
  <div class="card fc">
    <Icon name="box" />
    <div class="grow"><b class="mono">{name}</b><br /><span class="hint">{line}</span></div>
    {#if f.stage === 'checking'}<span class="spin"></span>
    {:else if f.stage === 'extracting'}<Badge tone="ok" icon="check">{p?.version ?? ''}</Badge>
    {:else if code === 'apk_wrong_app'}<Badge tone="bad" icon="x">{t('gf.wrongApp')}</Badge>
    {:else if code === 'apk_unsupported'}<Badge tone="warn" icon="alert">{t('gf.notSupported')}</Badge>
    {:else}<Badge tone="bad" icon="x">{t('gf.damaged')}</Badge>{/if}
  </div>
{/snippet}

<Dialog {title} icon={ok ? 'check' : f.stage === 'error' ? 'alert' : 'box'} tone={ok ? 'ok' : f.stage === 'error' ? errTone : 'p'} alert={f.stage === 'error'} onclose={() => f.close()}>
  {#if f.stage === 'ask'}
    {#if f.missing}
      <p>{t('gf.missingText')}</p>
      <p class="mono path">{store.game?.apk_path ?? ''}</p>
    {:else}
      <p>{t('gf.askText1')} <b>{t('gf.askOwn')}</b> {t('gf.askText2')}</p>
      <ul class="ol">
        <li>{t('gf.b1')}</li>
        <li>{t('gf.b2')}</li>
        <li>{t('gf.b3')}</li>
      </ul>
      <div class="sw" aria-hidden="true">{#each SWATCH as c (c)}<span style:background={c}></span>{/each}</div>
    {/if}
    {@render dropzone()}
    {#if phone}
      <div class="kpi"><span class="hint">{t('gf.noApk')}</span><Button size="sm" variant="ghost" icon="phone" onclick={() => void f.fromDevice(phone.id)}>{t('gf.fromPhone', { name: phone.name })}</Button></div>
    {/if}
  {:else if f.stage === 'checking'}
    {#if p?.stage === 'apk_pulling'}
      <Progress title={t('gf.pulling', { name: p.item ?? '' })} value={pct(p.done, p.total)} />
    {:else}
      {@render file(p?.item ?? '', p ? t('gf.fileLine', { size: bytes(p.total ?? 0), folder: dirName(p.path ?? '') }) : '')}
      <ul class="checks">
        {#each checks as c, i (c)}
          {#if i < (p?.index ?? 0)}<li><Icon name="check" />{t(c)}</li>
          {:else if i === (p?.index ?? 0)}<li><span class="spin"></span><b>{t(c)}</b></li>{/if}
        {/each}
      </ul>
    {/if}
  {:else if f.stage === 'extracting'}
    {@render file(baseName(store.game?.apk_path ?? 'TheBlockheads.apk'), t('gf.supported', { v: p?.version ?? '' }))}
    <div class="card pad">
      <Progress title={t('gf.extractingBar')} value={pct(p?.done, p?.total)}
        detail={t('gf.extractDetail', { what: t(`gf.part.${p?.item ?? 'blocks'}` as Key), done: p?.done ?? 0, total: p?.total ?? 0, eta: eta(p?.eta ?? 0) })} />
    </div>
    <p class="hint">{t('gf.savedLocal')}</p>
  {:else if f.stage === 'ready'}
    <div class="res" role="status">
      <span class="ri"><Icon name="check" size={28} /></span>
      <div><b>{t('gf.readyTitle')}</b><p class="muted">{t('gf.readyText', { v: store.game?.version ?? '', n: store.game?.images ?? 0, size: bytes(store.game?.size ?? 0) })}</p></div>
    </div>
  {:else}
    {@render file(f.error?.name ?? baseName(store.game?.apk_path ?? 'TheBlockheads.apk'), code === 'apk_unsupported' ? t('gf.versionLine', { v: f.error?.version ?? '?' }) : code === 'apk_damaged' ? t('gf.damagedLine') : '')}
    {#if code === 'apk_wrong_app'}
      <p>{t('gf.wrongText')}</p>
      {@render dropzone()}
    {:else if code === 'apk_unsupported'}
      <dl class="kv"><dt>{t('gf.thisFile')}</dt><dd>{f.error?.version ?? '?'}</dd><dt>{t('err.supported')}</dt><dd>1.7.4 – 1.7.6</dd></dl>
      <p>{t('gf.unsupportedText')}</p>
    {:else}
      <p>{code === 'apk_damaged' ? t('gf.damagedText') : t(`err.${code}` as Key)}</p>
      {#if phone}
        <Notice icon="phone" title={t('gf.wholeTitle')}>
          {t('gf.wholeText')}
          {#snippet actions()}<Button size="sm" icon="phone" onclick={() => void f.fromDevice(phone.id)}>{t('gf.fromPhone', { name: phone.name })}</Button>{/snippet}
        </Notice>
      {/if}
      {#if f.error?.detail}<pre class="log"><span class="e">{f.error.detail}</span></pre>{/if}
    {/if}
  {/if}
  {#snippet footer()}
    {#if f.stage === 'checking' || f.stage === 'extracting'}
      <Button onclick={() => f.close()}>{t('common.cancel')}</Button>
    {:else if f.stage === 'ready'}
      <Button variant="ghost" icon="sliders" onclick={() => { f.close(); store.go('settings'); }}>{t('nav.settings')}</Button>
      {#if f.world}
        <Button variant="primary" icon="pencil" onclick={() => f.openWorld()}>{t('gf.open', { name: f.world.name })}</Button>
      {:else}
        <Button variant="primary" onclick={() => f.close()}>{t('common.done')}</Button>
      {/if}
    {:else}
      {#if f.stage === 'error' && code !== 'apk_wrong_app' && code !== 'apk_unsupported'}<Button variant="ghost" icon="copy" onclick={() => void details()}>{t('common.copyDetails')}</Button>{/if}
      <Button variant="ghost" onclick={() => f.close()}>{t('common.notNow')}</Button>
      <Button variant="primary" icon="folder" onclick={() => void f.choose()}>{f.stage === 'error' ? t('gf.chooseOther') : t('gf.choose')}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .sw { display: flex; gap: 6px; flex-wrap: wrap; }
  .sw span { width: 28px; height: 28px; border-radius: 6px; box-shadow: 0 2px 0 rgba(0, 0, 0, 0.18); }
  .drop { gap: 6px; }
  .fc { padding: 12px 14px; display: flex; gap: 12px; align-items: center; }
  .grow { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .checks { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .checks li { display: flex; gap: 10px; align-items: center; }
  .path { overflow-wrap: anywhere; }
</style>
