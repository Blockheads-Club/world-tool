<script lang="ts">
  import { bytes, eta, pct, when } from '../format';
  import { t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { Device } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Check from '../ui/Check.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import PhoneWait from '../ui/PhoneWait.svelte';
  import Progress from '../ui/Progress.svelte';
  import Seg from '../ui/Seg.svelte';
  import Steps from '../ui/Steps.svelte';
  import Thumb from '../ui/Thumb.svelte';

  const f = store.read;
  const d = $derived(f.device as Device);
  const sub = $derived(
    [d.kind === 'pc' ? null : d.model, d.kind === 'android' ? (d.connection === 'wifi' ? 'Wi-Fi' : 'USB') : null, d.game_version ? t('read.gameV', { v: d.game_version }) : null]
      .filter(Boolean)
      .join(' · ')
  );
  const all = $derived((f.result?.worlds.length ?? 0) > 0 && f.selected.length === f.result?.worlds.length);
  const some = $derived(f.selected.length > 0 && !all);
  const p = $derived(f.progress);
</script>

<Header title={t('read.title', { name: d.name })} {sub}>
  {#snippet actions()}
    {#if d.read_only}<Badge tone="info" icon="eye">{t('dev.readOnly')}</Badge>{:else}<Badge tone="ok" dot>{t('dev.ready')}</Badge>{/if}
  {/snippet}
</Header>
<div class="main">
  <Steps steps={f.steps} current={f.stepIndex} />

  {#if f.stage === 'prepare'}
    <div class="grid2">
      <form class="card pad" onsubmit={(e) => { e.preventDefault(); void f.begin(); }}>
        <h2>{t('read.before')}</h2>
        {#if f.askClosed}
          <Notice tone="warn" title={t('read.closeTitle')}>{t('read.closeText')}</Notice>
          <Check bind:checked={f.gameClosed} label={t('common.gameClosed')} />
        {:else if d.kind === 'pc'}
          <Notice tone="ok" icon="check" title={t('read.pcChecked')}>{t('read.pcCheckedText')}</Notice>
        {/if}
        {#if d.kind === 'android' && f.needsPhone}
          <div class="ffield">
            <label for="bpw">{t('read.pw')}</label>
            <input id="bpw" class="input" type="password" bind:value={f.password} placeholder={t('read.pwPlaceholder')} aria-describedby="bpwh" />
            <span class="hint" id="bpwh">{t('read.pwHint')}</span>
          </div>
        {/if}
        <div class="kpi">
          <Button type="submit" variant="primary" size="lg" icon="download" disabled={f.askClosed && !f.gameClosed}>{t('read.start')}</Button>
          <Button variant="ghost" size="lg" onclick={() => store.go('devices')}>{t('common.cancel')}</Button>
        </div>
      </form>
      <div class="card pad">
        <h2>{t('read.next')}</h2>
        <ol class="ol">
          {#if f.needsPhone}<li>{t('read.next1')}</li>{/if}
          <li>{t('read.next2')}</li>
          <li>{t('read.next3')}</li>
        </ol>
        <p class="hint">{t('read.nextHint')}</p>
      </div>
    </div>
  {:else if f.stage === 'phone'}
    <PhoneWait screen="backup" title={t('read.phoneTitle')} text={t('read.phoneText')} elapsed={p?.elapsed ?? 0}>
      {#snippet actions()}
        <Button icon="x" onclick={() => void f.cancel()}>{t('common.cancel')}</Button>
        <Button variant="ghost" onclick={() => store.openGuide('trouble')}>{t('read.nothing')}</Button>
      {/snippet}
    </PhoneWait>
  {:else if f.stage === 'reading'}
    <div class="card pad">
      <Progress title={t('read.reading')} value={pct(p?.done, p?.total)}
        detail={p?.total ? t('read.readingDetail', { done: bytes(p.done ?? 0), total: bytes(p.total), eta: eta(p.eta ?? 0) }) : ''}>
        {#snippet actions()}<Button size="sm" onclick={() => void f.cancel()}>{t('common.cancel')}</Button>{/snippet}
      </Progress>
    </div>
    <Notice title={t('read.otherApps')}>{t('read.otherAppsText')}</Notice>
  {:else if f.stage === 'choose'}
    <div class="tools">
      <div class="srch"><Icon name="search" /><input class="input" type="search" placeholder={t('worlds.search')} aria-label={t('worlds.search')} bind:value={f.query} /></div>
      <Seg label={t('read.view')} size="sm" value={f.view} onchange={(v) => (f.view = v)} options={[{ value: 'list', label: t('read.list'), icon: 'list' }, { value: 'grid', label: t('read.grid'), icon: 'grid' }]} />
      <span class="hint">{t('read.count', { n: f.result?.worlds.length ?? 0 })} · {t('read.justNow')}</span>
    </div>
    {#if f.view === 'list'}
      <div class="card scroll">
        <table class="tbl" aria-label={t('read.tableAria', { name: d.name })}>
          <thead>
            <tr>
              <th><Check hideLabel label={t('read.selectAll')} checked={all} mixed={some} onchange={(v) => f.toggleAll(v)} /></th>
              <th>{t('read.map')}</th><th>{t('read.world')}</th><th>{t('read.size')}</th><th>{t('read.lastPlayed')}</th><th>{t('read.created')}</th><th>{t('read.mode')}</th>
            </tr>
          </thead>
          <tbody>
            {#each f.shown as w (w.id)}
              <tr class={[f.selected.includes(w.id) && 'rsel']}>
                <td><Check hideLabel label={w.name} checked={f.selected.includes(w.id)} onchange={(v) => f.toggle(w.id, v)} /></td>
                <td><Thumb src={w.preview} name={w.name} /></td>
                <td><b>{w.name}</b><br /><span class="hint">{t('read.gameV', { v: w.game_version })}</span></td>
                <td class="mono">{bytes(w.size)}</td>
                <td>{w.last_played ? when(w.last_played) : t('read.never')}</td>
                <td>{when(w.created)}</td>
                <td>{#if w.mode === 'expert'}<Badge tone="warn">{t('mode.expert')}</Badge>{:else}<Badge>{t('mode.normal')}</Badge>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="lgrid">
        {#each f.shown as w (w.id)}
          <div class={['wcard', f.selected.includes(w.id) && 'on']}>
            <Thumb src={w.preview} name={w.name} large />
            <Check label={w.name} checked={f.selected.includes(w.id)} onchange={(v) => f.toggle(w.id, v)} />
            <span class="hint">{bytes(w.size)} · {w.mode === 'expert' ? t('mode.expert') : t('mode.normal')}</span>
          </div>
        {/each}
      </div>
    {/if}
    <div class="card bar2">
      <b>{t('read.selected', { n: f.selected.length, size: bytes(f.selectedSize) })}</b>
      <span class="hint">{t('read.alsoSaved')}</span>
      <span class="sp"></span>
      <Button icon="library" disabled={f.selected.length === 0} onclick={() => void f.saveOnly()}>{t('read.saveOnly')}</Button>
      <Button variant="primary" icon="upload" disabled={f.selected.length === 0} onclick={() => void f.options()}>{t('read.exportN', { n: f.selected.length })}</Button>
    </div>
  {:else if f.stage === 'options'}
    <div class="grid2">
      <div class="radios" role="radiogroup" aria-label={t('read.exportAs')}>
        {#each [{ v: 'bhworld', title: t('read.asBhworld'), text: t('read.asBhworldText'), rec: true }, { v: 'server', title: t('read.asServer'), text: t('read.asServerText'), rec: false }] as o (o.v)}
          <label class={['radio', f.format === o.v && 'on']}>
            <input type="radio" name="fmt" value={o.v} checked={f.format === o.v} onchange={() => (f.format = o.v as 'bhworld' | 'server')} />
            <span class="rd" aria-hidden="true"></span>
            <span class="rt"><b>{o.title} {#if o.rec}<Badge tone="ok">{t('read.recommended')}</Badge>{/if}</b><span class="muted">{o.text}</span></span>
          </label>
        {/each}
      </div>
      <div class="card pad">
        <div class="ffield">
          <label for="dst">{t('read.saveTo')}</label>
          <div class="pathbox"><input id="dst" class="input" bind:value={f.folder} /><Button icon="folder" onclick={() => void f.browse()}>{t('common.browse')}</Button></div>
        </div>
        <b>{t('read.files')}</b>
        <ul class="files">{#each f.selectedWorlds as w (w.id)}<li class="mono">{f.fileName(w)}</li>{/each}</ul>
        <span class="hint">{bytes(f.selectedSize)}{#if f.free !== null} · {t('read.free', { size: bytes(f.free) })}{/if}</span>
        <div class="kpi">
          <Button variant="primary" size="lg" icon="upload" disabled={!f.folder.trim()} onclick={() => void f.exportNow()}>{t('read.export')}</Button>
          <Button variant="ghost" size="lg" onclick={() => (f.stage = 'choose')}>{t('common.back')}</Button>
        </div>
      </div>
    </div>
  {:else if f.stage === 'exporting'}
    <div class="card pad">
      <Progress title={t('read.exporting', { n: f.selected.length })} value={pct(p?.done, p?.total)}
        detail={p?.item ? t(p.stage === 'preview' ? 'read.exportDetailPreview' : 'read.exportDetail', { item: p.item, i: p.index ?? 1, n: p.count ?? 1 }) : ''}>
        {#snippet actions()}<Button size="sm" onclick={() => void f.cancel()}>{t('common.cancel')}</Button>{/snippet}
      </Progress>
    </div>
  {:else if f.exported}
    <div class="card pad" role="status">
      <div class="res">
        <span class="ri"><Icon name="check" size={28} /></span>
        <div><h2>{t('read.doneTitle', { n: f.exported.files.length })}</h2><p class="muted">{t('read.doneText', { folder: f.exported.folder })}</p></div>
      </div>
      <ul class="done">
        {#each f.exported.files as x (x.name)}
          <li>{#if f.format === 'bhworld'}<img src="/bhworld-file.svg" alt="" width="32" height="32" />{:else}<Icon name="server" />{/if}<span class="mono">{x.name}</span><span class="hint">{bytes(x.size)}</span></li>
        {/each}
      </ul>
      <div class="kpi">
        <Button variant="primary" icon="folder" onclick={() => void store.api.showInFolder(f.exported?.folder ?? '')}>{t('common.showInFolder')}</Button>
        <Button icon="library" onclick={() => store.go('worlds')}>{t('read.openWorlds')}</Button>
        <Button variant="ghost" onclick={() => store.go('devices')}>{t('common.done')}</Button>
      </div>
    </div>
  {/if}
</div>

<style>
  .tools { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .scroll { overflow-x: auto; }
  .bar2 { padding: 14px 18px; display: flex; gap: 12px; align-items: center; flex-wrap: wrap; position: sticky; bottom: 0; box-shadow: var(--lift); }
  .sp { flex: 1; }
  .lgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 14px; }
  .wcard { display: flex; flex-direction: column; gap: 8px; padding: 10px 10px 12px; border: 2px solid var(--line); border-radius: 16px; background: var(--surface); }
  .wcard.on { border-color: var(--primary); }
  .radios { display: flex; flex-direction: column; gap: 12px; }
  .radio { display: flex; gap: 12px; align-items: flex-start; padding: 14px 16px; border: 2px solid var(--line); border-radius: 14px; background: var(--surface); cursor: pointer; position: relative; }
  .radio.on { border-color: var(--primary); background: var(--primary-soft); }
  .radio input { position: absolute; opacity: 0; }
  .rd { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--line-strong); flex: none; margin-top: 2px; background: var(--surface); }
  .radio.on .rd { border: 7px solid var(--primary); }
  .radio:has(input:focus-visible) { outline: 3px solid var(--focus); outline-offset: 2px; }
  .rt { display: flex; flex-direction: column; gap: 4px; }
  .rt b { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .files { margin: 0; padding-left: 20px; }
  .done { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .done li { display: flex; gap: 10px; align-items: center; }
</style>
