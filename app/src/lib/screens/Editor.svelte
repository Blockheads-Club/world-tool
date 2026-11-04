<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Empty from '../ui/Empty.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import Seg from '../ui/Seg.svelte';
  import { viewTiles, zoomPercent } from '../editor/camera';
  import { TOOLS, type Panel, type Tool } from '../editor/flow.svelte';
  import GoToDialog from '../editor/GoToDialog.svelte';
  import HistoryPanel from '../editor/HistoryPanel.svelte';
  import ItemPicker from '../editor/ItemPicker.svelte';
  import MapView from '../editor/MapView.svelte';
  import Minimap from '../editor/Minimap.svelte';
  import { tileName } from '../editor/names';
  import type { Layers } from '../editor/paint';
  import PlayersPanel from '../editor/PlayersPanel.svelte';
  import SaveDialog from '../editor/SaveDialog.svelte';
  import SearchPanel from '../editor/SearchPanel.svelte';
  import TilePanel from '../editor/TilePanel.svelte';
  import ToolPanel from '../editor/ToolPanel.svelte';
  import WorldPanel from '../editor/WorldPanel.svelte';
  import type { EditorWorld } from '../types';

  /** World editor (WT-Editor): map, tools, palette, selection, paste, replace, history, players, world, search. */
  const ed = $derived(store.editor);
  const w = $derived(ed.world);
  const LAYERS: (keyof Layers)[] = ['blocks', 'walls', 'liquids', 'light', 'objects'];
  const PANELS: Panel[] = ['tile', 'history', 'players', 'world', 'search'];
  const view = $derived(viewTiles(ed.cam, ed.size));
  const hoverTile = $derived.by(() => {
    const h = ed.hover as { x: number; y: number };
    const b = ed.blocks.get(`${Math.floor(h.x / 32)},${Math.floor(h.y / 32)}`);
    void ed.version;
    if (!b?.data) return t('edv.notGenerated');
    const i = ((h.y % 32) * 32 + (h.x % 32)) * 4;
    return tileName(ed.catalog, b.data[i] === 2 && b.data[i + 1] !== 2 ? b.data[i + 1] : b.data[i]);
  });
  const title = $derived(ed.ed.name ?? (ed.source ? ed.source.name : ''));
  const toolLabel = (tool: Tool) => t(`edv.tool.${tool}`);

  /** A game version the editor was not made for, else `null`. */
  function odd(world: EditorWorld): string | null {
    const v = world.game_version;
    return v !== null && !v.startsWith('1.7') ? v : null;
  }

  function dialogOpen() {
    return ed.goToOpen || ed.saveAsk || ed.itemPick !== null || ed.teleport !== null || ed.removeAsk !== null;
  }

  function keydown(e: KeyboardEvent) {
    if (ed.status !== 'ready' || dialogOpen()) return;
    const target = e.target;
    if (target instanceof Element && target.closest('input, textarea, select, [role=radiogroup]')) return;
    const k = e.key.toLowerCase();
    const mod = e.ctrlKey || e.metaKey;
    if (mod && (k === 'y' || (k === 'z' && e.shiftKey))) void ed.redo();
    else if (mod && k === 'z') void ed.undo();
    else if (mod && k === 'c') void ed.copy();
    else if (mod && k === 'x') void ed.copy(true);
    else if (mod && k === 'v') ed.setTool('paste');
    else if (mod && k === 's') ed.saveAsk = ed.ed.unsaved > 0;
    else if (mod) return;
    else if (e.key === 'F11') ed.fullscreen = !ed.fullscreen;
    else if (e.key === 'Escape') {
      if (ed.paste) ed.setTool('inspect');
      else if (ed.selection) ed.selection = null;
      else if (ed.fullscreen) ed.fullscreen = false;
      else return;
    } else if (e.key === 'Tab' && ed.fullscreen) ed.panelsHidden = !ed.panelsHidden;
    else if (e.key === 'Delete' && ed.selection) void ed.clearSelection();
    else if (k === 'g') ed.goToOpen = true;
    else if (e.key === 'Home') ed.goSpawn();
    else {
      const tool = TOOLS.find((x) => x.key === k);
      if (!tool) return;
      ed.setTool(tool.tool);
    }
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class={['ed', ed.fullscreen && 'full', ed.panelsHidden && 'bare']}>
  <div class="etop">
    <Button size="sm" variant="ghost" icon="back" label={t('edv.back')} onclick={() => ed.leave()}>{t('nav.worlds')}</Button>
    <div class="wn">
      <b>{title}</b>
      <span class="hint">{t('edv.editing')}</span>
    </div>
    {#if ed.ed.unsaved > 0}<Badge tone="warn">{t('edv.unsaved', { n: ed.ed.unsaved })}</Badge>{/if}
    {#if w}
      <div class="tgroup" role="group" aria-label={t('edv.panel.history')}>
        <button class="tbtn" type="button" aria-label={t('edv.undo')} title={t('edv.undo')} disabled={ed.ed.cursor === 0} onclick={() => ed.undo()}><Icon name="undo" /></button>
        <button class="tbtn" type="button" aria-label={t('edv.redo')} title={t('edv.redo')} disabled={ed.ed.cursor >= ed.ed.history.length} onclick={() => ed.redo()}><Icon name="redo" /></button>
      </div>
      <div class="tgroup" role="toolbar" aria-label={t('edv.tools')}>
        {#each TOOLS as x (x.tool)}
          <button class="tbtn" type="button" aria-pressed={ed.tool === x.tool} aria-label={toolLabel(x.tool)} title={toolLabel(x.tool)} onclick={() => ed.setTool(x.tool)}><Icon name={x.icon} /></button>
        {/each}
      </div>
      <button class="tbtn" type="button" aria-pressed={ed.fullscreen} aria-label={t('edv.fullscreen')} title={t('edv.fullscreen')} onclick={() => (ed.fullscreen = !ed.fullscreen)}><Icon name="maximize" /></button>
      <span class="grow"></span>
      <Button size="sm" variant="primary" icon="save" disabled={ed.ed.unsaved === 0} loading={ed.saving} onclick={() => ed.save(false)}>{t('edv.saveWorlds')}</Button>
      <Button size="sm" loading={ed.saving} onclick={() => ed.save(true)}>{t('edv.saveNew')}</Button>
    {/if}
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
    {#if odd(w) || w.damaged_blocks > 0}
      <div class="notes">
        {#if odd(w)}<Notice tone="warn" title={t('edv.version.title', { v: `${odd(w)}` })}>{t('edv.version.text')}</Notice>{/if}
        {#if w.damaged_blocks > 0}<Notice tone="warn" title={t('edv.damaged', { n: w.damaged_blocks })} />{/if}
      </div>
    {/if}
    <div class="ebody">
      <aside class="epanel" aria-label={t('edv.toolOptions')}><ToolPanel /></aside>
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
        {#if ed.fullscreen}
          <div class="float fsbar" role="status">{t('edv.fullHint')}<Button size="sm" onclick={() => (ed.fullscreen = false)}>{t('edv.exitFull')}</Button></div>
        {/if}
        <Minimap width={w.width} height={w.height} />
      </div>
      <aside class="epanel r" aria-label={t('edv.panel')}>
        <Seg label={t('edv.panel')} size="sm" value={ed.panel} options={PANELS.map((p) => ({ value: p, label: t(`edv.panel.${p}`) }))} onchange={(v) => (ed.panel = v)} />
        {#if ed.panel === 'tile'}<TilePanel />{:else if ed.panel === 'history'}<HistoryPanel />{:else if ed.panel === 'players'}<PlayersPanel />{:else if ed.panel === 'world'}<WorldPanel />{:else}<SearchPanel />{/if}
      </aside>
    </div>
    <div class="estat" role="status">
      <span>{t(`edv.hint.${ed.tool}`)}</span>
      <span>{t('edv.status.view', { w: view.w, h: view.h })}</span>
      <span>{ed.atlas ? t('edv.status.gfxGame', { v: `${store.game!.version}` }) : t('edv.status.gfxPlain')}</span>
      {#if ed.busy}<span class="spin small" aria-label={t('common.loading')}></span>{/if}
    </div>
    {#if ed.goToOpen}
      <GoToDialog title={t('edv.goto.title')} action={t('edv.goto.go')} start={[Math.floor(ed.cam.x), Math.floor(ed.cam.y)]} onclose={() => (ed.goToOpen = false)} onpick={(x, y) => { ed.goToOpen = false; ed.goTo(x, y); }} />
    {/if}
    {#if ed.teleport !== null}
      {@const b = ed.blockheads[ed.teleport]}
      <GoToDialog title={t('edv.teleportTitle', { name: b.name })} action={t('edv.teleport')} start={[b.x, b.y]} onclose={() => (ed.teleport = null)} onpick={(x, y) => { const { player, id } = b; ed.teleport = null; void ed.edit({ op: 'move_blockhead', player, id, x, y }); }} />
    {/if}
    {#if ed.removeAsk !== null}
      {@const b = ed.blockheads[ed.removeAsk]}
      <Dialog title={t('edv.removeTitle', { name: b.name })} icon="trash" tone="bad" alert onclose={() => (ed.removeAsk = null)}>
        <p>{t('edv.removeText')}</p>
        {#snippet footer()}
          <Button variant="ghost" onclick={() => (ed.removeAsk = null)}>{t('common.cancel')}</Button>
          <Button variant="danger" onclick={() => { const { player, id } = b; ed.removeAsk = null; ed.picked = null; void ed.edit({ op: 'remove_blockhead', player, id }); }}>{t('edv.removePlayer')}</Button>
        {/snippet}
      </Dialog>
    {/if}
    {#if ed.itemPick}<ItemPicker />{/if}
    {#if ed.saveAsk}<SaveDialog name={title} />{/if}
  {/if}
</div>

<style>
  .ed { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .ed.full { position: fixed; inset: 0; z-index: 50; background: var(--bg); }
  .ed.full .estat, .ed.bare .epanel { display: none; }
  .etop { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-bottom: 2px solid var(--line); background: var(--surface); flex-wrap: wrap; }
  .wn { display: flex; flex-direction: column; line-height: 1.2; margin-right: 4px; min-width: 0; }
  .wn b { font: 800 17px var(--f-display); }
  .hint { font-size: 13px; color: var(--text-3); white-space: nowrap; }
  .grow { flex: 1; }
  .tgroup { display: flex; gap: 2px; padding: 3px; border-radius: 12px; background: var(--surface-2); border: 2px solid var(--line); }
  .tbtn { width: 40px; height: 40px; border-radius: 9px; border: 0; background: transparent; color: var(--text-2); display: grid; place-items: center; cursor: pointer; }
  .tbtn[aria-pressed='true'] { background: var(--text); color: var(--bg); }
  .tbtn[disabled] { opacity: 0.4; cursor: default; }
  .center { flex: 1; display: grid; place-items: center; padding: 24px; }
  .card { display: flex; gap: 14px; align-items: center; padding: 20px 24px; background: var(--surface); border: 2px solid var(--line); border-radius: 16px; max-width: 520px; }
  .card p { white-space: normal; }
  .notes { display: flex; flex-direction: column; gap: 8px; padding: 10px 14px 0; }
  .ebody { flex: 1; min-height: 0; display: flex; }
  .canvas { flex: 1; min-width: 0; position: relative; overflow: hidden; background-color: #8fcbea; }
  .epanel { width: 250px; flex: none; border-right: 2px solid var(--line); background: var(--surface); overflow: auto; padding: 14px; display: flex; flex-direction: column; gap: 14px; }
  .epanel.r { width: 310px; border-right: 0; border-left: 2px solid var(--line); }
  :global(.ed .float) { position: absolute; background: color-mix(in oklab, var(--surface), transparent 6%); border: 2px solid var(--line-strong); border-radius: 12px; box-shadow: var(--lift); padding: 8px 10px; font-size: 13.5px; }
  .layers { left: 12px; top: 12px; display: flex; gap: 6px; flex-wrap: wrap; max-width: calc(100% - 24px); }
  .lt { display: inline-flex; gap: 6px; align-items: center; min-height: 32px; padding: 0 10px; border-radius: 999px; border: 2px solid var(--line-strong); background: var(--surface); font: 700 12.5px var(--f-body); color: var(--text-2); cursor: pointer; }
  .lt[aria-pressed='true'] { background: var(--text); border-color: var(--text); color: var(--bg); }
  .coord { left: 12px; bottom: 12px; display: flex; gap: 12px; align-items: center; font: 600 13px var(--f-mono); }
  .coord .nm { font-family: var(--f-body); }
  .zoom { right: 12px; bottom: 170px; display: flex; flex-direction: column; gap: 4px; padding: 4px; }
  .fsbar { left: 50%; top: 12px; transform: translateX(-50%); display: flex; gap: 10px; align-items: center; }
  .estat { display: flex; gap: 14px; align-items: center; padding: 6px 14px; border-top: 2px solid var(--line); background: var(--surface); font-size: 13px; color: var(--text-2); flex-wrap: wrap; }
  .spin.small { width: 14px; height: 14px; border-width: 2px; }
  @media (max-width: 1250px) { .epanel { width: 220px; } .epanel.r { width: 270px; } }
</style>
