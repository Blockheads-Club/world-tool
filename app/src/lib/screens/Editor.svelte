<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Empty from '../ui/Empty.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import Seg from '../ui/Seg.svelte';
  import { viewTiles, zoomPercent } from '../editor/camera';
  import type { Panel } from '../editor/flow.svelte';
  import type { EditorWorld } from '../types';
  import GoToDialog from '../editor/GoToDialog.svelte';
  import MapView from '../editor/MapView.svelte';
  import Minimap from '../editor/Minimap.svelte';
  import { tileName } from '../editor/names';
  import type { Layers } from '../editor/paint';
  import PlayersPanel from '../editor/PlayersPanel.svelte';
  import TilePanel from '../editor/TilePanel.svelte';
  import WorldPanel from '../editor/WorldPanel.svelte';

  /** World editor (WT-Editor): read-only viewer so far — map, minimap, go to, layers, inspector, players, world. */
  const ed = $derived(store.editor);
  const w = $derived(ed.world);
  const LAYERS: (keyof Layers)[] = ['blocks', 'walls', 'liquids', 'light', 'objects'];
  const view = $derived(viewTiles(ed.cam, ed.size));
  const hoverTile = $derived.by(() => {
    const h = ed.hover as { x: number; y: number };
    const b = ed.blocks.get(`${Math.floor(h.x / 32)},${Math.floor(h.y / 32)}`);
    void ed.version;
    if (!b?.data) return t('edv.notGenerated');
    const i = ((h.y % 32) * 32 + (h.x % 32)) * 4;
    return tileName(ed.catalog, b.data[i] === 2 && b.data[i + 1] !== 2 ? b.data[i + 1] : b.data[i]);
  });
  const title = $derived(w ? w.name : ed.source ? ed.source.name : '');
  /** A game version the editor was not made for, else `null`. */
  function odd(world: EditorWorld): string | null {
    const v = world.game_version;
    return v !== null && !v.startsWith('1.7') ? v : null;
  }

  function keydown(e: KeyboardEvent) {
    if (ed.status !== 'ready' || ed.goToOpen) return;
    const target = e.target;
    if (target instanceof Element && target.closest('input, textarea, [role=radiogroup]')) return;
    if (e.key === 'g' || e.key === 'G') ed.goToOpen = true;
    else if (e.key === 'Home') ed.goSpawn();
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="ed">
  <div class="etop">
    <Button size="sm" variant="ghost" icon="back" label={t('edv.back')} onclick={() => store.go('worlds')}>{t('nav.worlds')}</Button>
    <div class="wn">
      <b>{title}</b>
      <span class="hint">{t('edv.viewing')}</span>
    </div>
    <div class="tgroup" role="toolbar" aria-label={t('edv.layers')}>
      <button class="tbtn" type="button" aria-pressed="true" aria-label={t('edv.status.inspect')} title={t('edv.status.inspect')}><Icon name="cursor" /></button>
    </div>
    <span class="hint ro">{t('edv.readOnly')}</span>
  </div>

  {#if ed.status === 'opening'}
    <div class="center">
      <div class="card" role="status">
        <span class="spin"></span>
        <div><b>{t('edv.opening', { name: title })}</b><p class="hint">{t('edv.openingText')}</p></div>
      </div>
    </div>
  {:else if ed.status === 'error'}
    <div class="center">
      <Empty icon="alert" title={t('edv.openFailed')} text={[t(`err.${ed.error!.code}`), ed.error!.detail].filter(Boolean).join(' ')}>
        <Button variant="primary" icon="back" onclick={() => store.go('worlds')}>{t('edv.back')}</Button>
      </Empty>
    </div>
  {:else if w}
    <div class="notes">
      {#if odd(w)}<Notice tone="warn" title={t('edv.version.title', { v: `${odd(w)}` })}>{t('edv.version.text')}</Notice>{/if}
      {#if w.damaged_blocks > 0}<Notice tone="warn" title={t('edv.damaged', { n: w.damaged_blocks })} />{/if}
    </div>
    <div class="ebody">
      <div class="canvas">
        <MapView />
        <div class="float layers" role="group" aria-label={t('edv.layers')}>
          {#each LAYERS as l (l)}
            <button class="lt" type="button" aria-pressed={ed.layers[l]} onclick={() => ed.toggle(l)}>{t(`edv.layer.${l}`)}</button>
          {/each}
        </div>
        <div class="float coord" role="status">
          {#if ed.hover}<span>{`X ${ed.hover.x}`}</span><span>{`Y ${ed.hover.y}`}</span><span class="nm">{`${hoverTile}`}</span>{/if}
          <span>{t('edv.zoom', { n: zoomPercent(ed.cam.zoom) })}</span>
        </div>
        <div class="float zoom" role="group" aria-label={t('edv.zoom', { n: zoomPercent(ed.cam.zoom) })}>
          <button class="tbtn" type="button" aria-label={t('edv.zoomIn')} title={t('edv.zoomIn')} onclick={() => ed.zoom(1)}><Icon name="zoomin" /></button>
          <button class="tbtn" type="button" aria-label={t('edv.zoomOut')} title={t('edv.zoomOut')} onclick={() => ed.zoom(-1)}><Icon name="zoomout" /></button>
          <button class="tbtn" type="button" aria-label={t('edv.goTo')} title={t('edv.goTo')} onclick={() => (ed.goToOpen = true)}><Icon name="target" /></button>
          <button class="tbtn" type="button" aria-label={t('edv.spawn')} title={t('edv.spawn')} disabled={!w.spawn} onclick={() => ed.goSpawn()}><Icon name="portal" /></button>
        </div>
        <Minimap width={w.width} height={w.height} />
      </div>
      <aside class="epanel r" aria-label={t('edv.panel')}>
        <Seg
          label={t('edv.panel')}
          size="sm"
          value={ed.panel}
          options={[
            { value: 'tile' as Panel, label: t('edv.panel.tile') },
            { value: 'players' as Panel, label: t('edv.panel.players') },
            { value: 'world' as Panel, label: t('edv.panel.world') }
          ]}
          onchange={(v) => (ed.panel = v)}
        />
        {#if ed.panel === 'tile'}<TilePanel />{:else if ed.panel === 'players'}<PlayersPanel />{:else}<WorldPanel />{/if}
      </aside>
    </div>
    <div class="estat" role="status">
      <span>{t('edv.status.inspect')}</span>
      <span>{t('edv.status.view', { w: view.w, h: view.h })}</span>
      <span>{ed.atlas ? t('edv.status.gfxGame', { v: `${store.game!.version}` }) : t('edv.status.gfxPlain')}</span>
    </div>
    {#if ed.goToOpen}<GoToDialog />{/if}
  {/if}
</div>

<style>
  .ed { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .etop { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-bottom: 2px solid var(--line); background: var(--surface); flex-wrap: wrap; }
  .wn { display: flex; flex-direction: column; line-height: 1.2; margin-right: 4px; min-width: 0; }
  .wn b { font: 800 17px var(--f-display); }
  .hint { font-size: 13px; color: var(--text-3); white-space: nowrap; }
  .ro { margin-left: auto; white-space: normal; }
  .tgroup { display: flex; gap: 2px; padding: 3px; border-radius: 12px; background: var(--surface-2); border: 2px solid var(--line); }
  .tbtn { width: 40px; height: 40px; border-radius: 9px; border: 0; background: transparent; color: var(--text-2); display: grid; place-items: center; cursor: pointer; }
  .tbtn[aria-pressed='true'] { background: var(--text); color: var(--bg); }
  .tbtn[disabled] { opacity: 0.4; cursor: default; }
  .center { flex: 1; display: grid; place-items: center; padding: 24px; }
  .card { display: flex; gap: 14px; align-items: center; padding: 20px 24px; background: var(--surface); border: 2px solid var(--line); border-radius: 16px; max-width: 520px; }
  .card p { white-space: normal; }
  .notes:empty { display: none; }
  .notes { display: flex; flex-direction: column; gap: 8px; padding: 10px 14px 0; }
  .ebody { flex: 1; min-height: 0; display: flex; }
  .canvas { flex: 1; min-width: 0; position: relative; overflow: hidden; background-color: #8fcbea; }
  .epanel { width: 300px; flex: none; border-left: 2px solid var(--line); background: var(--surface); overflow: auto; padding: 14px; display: flex; flex-direction: column; gap: 14px; }
  :global(.ed .float) { position: absolute; background: color-mix(in oklab, var(--surface), transparent 6%); border: 2px solid var(--line-strong); border-radius: 12px; box-shadow: var(--lift); padding: 8px 10px; font-size: 13.5px; }
  .layers { left: 12px; top: 12px; display: flex; gap: 6px; flex-wrap: wrap; max-width: calc(100% - 24px); }
  .lt { display: inline-flex; gap: 6px; align-items: center; min-height: 32px; padding: 0 10px; border-radius: 999px; border: 2px solid var(--line-strong); background: var(--surface); font: 700 12.5px var(--f-body); color: var(--text-2); cursor: pointer; }
  .lt[aria-pressed='true'] { background: var(--text); border-color: var(--text); color: var(--bg); }
  .coord { left: 12px; bottom: 12px; display: flex; gap: 12px; align-items: center; font: 600 13px var(--f-mono); }
  .coord .nm { font-family: var(--f-body); }
  .zoom { right: 12px; bottom: 170px; display: flex; flex-direction: column; gap: 4px; padding: 4px; }
  .estat { display: flex; gap: 14px; align-items: center; padding: 6px 14px; border-top: 2px solid var(--line); background: var(--surface); font-size: 13px; color: var(--text-2); flex-wrap: wrap; }
  @media (max-width: 1100px) { .epanel { width: 260px; } }
</style>
