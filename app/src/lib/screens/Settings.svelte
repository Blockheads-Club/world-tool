<script lang="ts">
  import { blank, bytes, dash, when } from '../format';
  import { LANGUAGES, i18n, t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { PairedDevice, RootMode, Settings, ThemeChoice } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Notice from '../ui/Notice.svelte';
  import Seg from '../ui/Seg.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';
  import LegalDialog from './LegalDialog.svelte';

  const s = $derived(store.settings as Settings);
  const g = $derived(store.game);
  const u = store.update;
  let paired = $state<PairedDevice[] | null>(null);
  let managing = $state(false);
  let legal = $state<'legal' | 'licences' | null>(null);
  let clearing = $state(false);

  $effect(() => {
    void store.api.pairedDevices().then((p) => (paired = p));
  });

  async function folder() {
    const f = await store.api.pickFolder(s.worlds_folder);
    if (f) await store.saveSettings({ worlds_folder: f });
  }

  async function forget(id: string) {
    await store.api.forgetPaired(id);
    paired = await store.api.pairedDevices();
  }

  async function diag() {
    toast((await copyText(await store.api.diagnostics())) ? t('common.copied') : t('common.copyFailed'));
  }

  async function clearGame() {
    clearing = false;
    store.game = await store.api.clearGameFiles();
  }
</script>

{#snippet row(title: string, hint: string, control: import('svelte').Snippet)}
  <div class="cf">
    <div class="ct"><b>{title}</b><span class="hint">{hint}</span></div>
    <div class="cc">{@render control()}</div>
  </div>
{/snippet}

<Header title={t('nav.settings')} />
<div class="main">
  {#if u.stage === 'available' && u.info}
    <Notice icon="upd" title={t('upd.title', { v: u.info.version })}>
      {u.info.notes[0] ?? ''}
      {#snippet actions()}
        <Button size="sm" variant="primary" icon="upd" onclick={() => void u.download()}>{t('upd.install')}</Button>
        <Button size="sm" variant="ghost" onclick={() => (u.popup = true)}>{t('upd.whatsNew')}</Button>
      {/snippet}
    </Notice>
  {/if}

  <section class="card" aria-labelledby="s-w">
    <h2 id="s-w">{t('nav.worlds')}</h2>
    {#snippet wf()}<div class="pathbox"><input class="input" value={s.worlds_folder} readonly aria-label={t('set.folder')} /><Button icon="folder" onclick={() => void folder()}>{t('common.change')}</Button></div>{/snippet}
    {@render row(t('set.folder'), t('set.folderHint'), wf)}
  </section>

  <section class="card" aria-labelledby="s-g">
    <h2 id="s-g">{t('set.game')}</h2>
    <div class="gf">
      <div class="gt">
        <b>{t('set.gameTitle')}</b>
        {#if g?.state === 'ready'}<Badge tone="ok" dot>{t('dev.ready')}</Badge>
        {:else if g?.state === 'unsupported'}<Badge tone="warn" icon="alert">{t('set.unsupported')}</Badge>
        {:else if g?.state === 'damaged'}<Badge tone="bad" icon="x">{t('gf.damaged')}</Badge>
        {:else if g?.state === 'missing'}<Badge tone="warn" icon="alert">{t('set.missing')}</Badge>
        {:else}<Badge icon="box">{t('set.notSet')}</Badge>{/if}
      </div>
      {#if g?.state === 'ready'}
        <p class="muted">{t('set.gameReady', { v: g.version ?? '', name: g.apk_name ?? '', n: g.images ?? 0, size: bytes(g.size ?? 0) })}</p>
        <div class="kpi">
          <Button size="sm" icon="folder" onclick={() => store.gameFiles.open()}>{t('set.replace')}</Button>
          <Button size="sm" variant="ghost" icon="refresh" onclick={() => void store.gameFiles.reextract()}>{t('set.reextract')}</Button>
          <Button size="sm" variant="ghost" icon="trash" onclick={() => (clearing = true)}>{t('set.clear')}</Button>
        </div>
      {:else if g?.state === 'unsupported' || g?.state === 'damaged'}
        <p class="muted">{g.state === 'unsupported' ? t('set.unsupportedText', { name: blank(g.apk_name), v: dash(g.version) }) : t('gf.damagedText')}</p>
        <div class="kpi">
          <Button size="sm" variant="primary" icon="folder" onclick={() => store.gameFiles.open()}>{t('set.replace')}</Button>
          <Button size="sm" variant="ghost" icon="trash" onclick={() => (clearing = true)}>{t('set.clear')}</Button>
        </div>
      {:else}
        <p class="muted">{g?.state === 'missing' ? t('gf.missingText') : t('set.notSetText')}</p>
        <div class="kpi">
          <Button size="sm" variant="primary" icon="folder" onclick={() => store.gameFiles.open()}>{t('gf.choose')}</Button>
          {#if store.readyPhones[0]}<Button size="sm" variant="ghost" icon="phone" onclick={() => { store.gameFiles.open(); void store.gameFiles.fromDevice(store.readyPhones[0].id); }}>{t('set.fromPhone')}</Button>{/if}
        </div>
      {/if}
      <p class="hint">{t('set.gameHint')}</p>
    </div>
  </section>

  <section class="card" aria-labelledby="s-b">
    <h2 id="s-b">{t('nav.backups')}</h2>
    {#snippet bw()}<Toggle hideLabel label={t('set.backupWrite')} checked={s.backup_before_write} onchange={(v) => void store.saveSettings({ backup_before_write: v })} />{/snippet}
    {@render row(t('set.backupWrite'), t('set.backupWriteHint'), bw)}
    {#snippet keep()}
      <select class="sel" aria-label={t('set.keep')} onchange={(e) => void store.saveSettings({ backups_to_keep: Number(e.currentTarget.value) })}>
        {#each [1, 2, 3, 5, 10, 20] as n (n)}<option selected={n === s.backups_to_keep}>{`${n}`}</option>{/each}
      </select>
    {/snippet}
    {@render row(t('set.keep'), t('set.keepHint'), keep)}
  </section>

  <section class="card" aria-labelledby="s-c">
    <h2 id="s-c">{t('set.connection')}</h2>
    {#snippet root()}
      <Seg label={t('set.root')} value={s.root_mode} onchange={(v: RootMode) => void store.saveSettings({ root_mode: v })}
        options={[{ value: 'auto', label: t('root.auto') }, { value: 'always', label: t('root.always') }, { value: 'never', label: t('root.never') }]} />
    {/snippet}
    {@render row(t('set.root'), t('set.rootHint'), root)}
    {#snippet pd()}<Button size="sm" onclick={() => (managing = true)}>{t('set.manage')}</Button>{/snippet}
    {@render row(t('set.paired'), paired ? t('set.pairedN', { n: paired.length }) : '', pd)}
  </section>

  <section class="card" aria-labelledby="s-a">
    <h2 id="s-a">{t('set.appearance')}</h2>
    {#snippet th()}
      <Seg label={t('set.theme')} value={s.theme} onchange={(v: ThemeChoice) => void store.saveSettings({ theme: v })}
        options={[{ value: 'system', label: t('set.system') }, { value: 'light', label: t('set.light') }, { value: 'dark', label: t('set.dark') }]} />
    {/snippet}
    {@render row(t('set.theme'), t('set.themeHint'), th)}
    {#snippet lang()}
      <select class="sel" aria-label={t('set.language')} onchange={(e) => void store.saveSettings({ lang: LANGUAGES[e.currentTarget.selectedIndex].code })}>
        {#each LANGUAGES as l (l.code)}<option selected={l.code === i18n.lang}>{`${l.label}`}</option>{/each}
      </select>
    {/snippet}
    {@render row(t('set.language'), 'English, Türkçe, Русский, Қазақша', lang)}
    {#snippet rm()}<Toggle hideLabel label={t('set.motion')} checked={s.reduce_motion} onchange={(v) => void store.saveSettings({ reduce_motion: v })} />{/snippet}
    {@render row(t('set.motion'), t('set.motionHint'), rm)}
  </section>

  <section class="card" aria-labelledby="s-u">
    <h2 id="s-u">{t('set.updates')}</h2>
    {#snippet au()}<Toggle hideLabel label={t('set.autoUpdate')} checked={s.auto_update} onchange={(v) => void store.saveSettings({ auto_update: v })} />{/snippet}
    {@render row(t('set.autoUpdate'), t('set.autoUpdateHint'), au)}
    {#snippet cn()}<Button size="sm" icon="refresh" loading={u.stage === 'checking'} onclick={() => void u.check()}>{t('set.checkNow')}</Button>{/snippet}
    {@render row(t('set.version', { v: store.info.version }), s.last_update_check ? t('set.checked', { when: when(s.last_update_check, Date.now(), true) }) : t('set.neverChecked'), cn)}
  </section>

  <section class="card" aria-labelledby="s-ab">
    <h2 id="s-ab">{t('set.about')}</h2>
    {#snippet lic()}<Button size="sm" variant="ghost" onclick={() => (legal = 'licences')}>{t('set.view')}</Button>{/snippet}
    {@render row(t('set.licences'), t('set.licencesHint'), lic)}
    {#snippet lg()}<Button size="sm" variant="ghost" onclick={() => (legal = 'legal')}>{t('set.read')}</Button>{/snippet}
    {@render row(t('legal.title'), t('set.legalHint'), lg)}
    {#snippet dg()}<Button size="sm" variant="ghost" icon="copy" onclick={() => void diag()}>{t('common.copyDiag')}</Button>{/snippet}
    {@render row(t('set.diag'), t('set.diagHint'), dg)}
  </section>
</div>

{#if managing}
  <Dialog title={t('set.paired')} icon="wifi" onclose={() => (managing = false)}>
    {#if paired?.length}
      <ul class="pl">
        {#each paired as p (p.id)}
          <li><span><b>{p.name}</b><br /><span class="hint mono">{p.address}</span></span><Button size="sm" variant="quiet-bad" icon="trash" onclick={() => void forget(p.id)}>{t('dev.forget')}</Button></li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{t('set.pairedNone')}</p>
    {/if}
    {#snippet footer()}
      <Button icon="plus" onclick={() => { managing = false; store.openGuide('wifi'); }}>{t('set.pairNew')}</Button>
      <Button variant="primary" onclick={() => (managing = false)}>{t('common.done')}</Button>
    {/snippet}
  </Dialog>
{/if}
{#if legal}<LegalDialog licences={legal === 'licences'} onclose={() => (legal = null)} />{/if}
{#if clearing}
  <Dialog title={t('set.clearTitle')} icon="trash" tone="bad" alert onclose={() => (clearing = false)}>
    <p>{t('set.clearText')}</p>
    {#snippet footer()}
      <Button onclick={() => (clearing = false)} autofocus>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" onclick={() => void clearGame()}>{t('set.clear')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  section h2 { font-size: 18px; padding: 16px 18px 4px; }
  .cf { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px 16px; align-items: center; padding: 12px 18px; border-top: 1px solid var(--line); }
  h2 + .cf { border-top: 0; }
  .ct { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .cc { display: flex; justify-content: flex-end; min-width: 0; }
  .cc .pathbox { width: min(520px, 50vw); }
  .gf { padding: 4px 18px 16px; display: flex; flex-direction: column; gap: 10px; }
  .gt { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .pl { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  .pl li { display: flex; justify-content: space-between; gap: 12px; align-items: center; }
  @media (max-width: 1100px) { .cf { grid-template-columns: minmax(0, 1fr); } .cc { justify-content: flex-start; } .cc .pathbox { width: 100%; } }
</style>
