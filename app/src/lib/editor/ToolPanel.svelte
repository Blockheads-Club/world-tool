<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Check from '../ui/Check.svelte';
  import Seg from '../ui/Seg.svelte';
  import type { Brush, Catalog, IdInfo, Layer } from '../types';
  import { brushName, contentsName, nameMatches, tileName } from './names';
  import { oreBrush } from './geom';
  import Swatch from './Swatch.svelte';

  /** Left panel: the block palette and the options of the current tool (brush, selection, paste, replace, schematics). */
  const ed = $derived(store.editor);
  const cat = $derived(ed.catalog);
  let query = $state('');
  let schName = $state('');

  /** Palette entries: tile types (air included), then plain ores and gems in rock. */
  const entries = $derived.by(() => {
    const c = cat as Catalog;
    const tiles = c.tiles.filter((x) => x.id > 0).map((x) => ({ brush: { layer: 'front' as Layer, tile: x.id, contents: 0 }, info: x, name: tileName(c, x.id), english: x.name }));
    const ores = c.contents
      .filter((x) => x.placeable)
      .map((x) => ({ brush: oreBrush(x.id), info: x as IdInfo, name: contentsName(c, x.id), english: x.name }));
    return [...tiles, ...ores].filter((e) => nameMatches(e.name, e.english, query));
  });

  const same = (a: Brush, b: Brush) => a.tile === b.tile && a.contents === b.contents;
  const blockName = $derived(brushName(cat, ed.block));

  function choose(b: Brush) {
    ed.block = { ...b, layer: ed.block.layer === 'back' && !b.contents ? 'back' : 'front' };
  }
</script>

<section aria-labelledby="pal-h">
  <h3 id="pal-h">{t('edv.blocks')}<span class="n">{`${entries.length}`}</span></h3>
  <input class="input" type="search" placeholder={t('edv.findBlock')} aria-label={t('edv.findBlock')} bind:value={query} />
  <Seg
    label={t('edv.layer')}
    size="sm"
    value={ed.block.layer}
    options={[
      { value: 'front' as Layer, label: t('edv.layerFront') },
      { value: 'back' as Layer, label: t('edv.layerBack') }
    ]}
    onchange={(v) => (ed.block = { ...ed.block, layer: v, contents: v === 'back' ? 0 : ed.block.contents })}
  />
  <div class="pal" role="listbox" aria-label={t('edv.palette')}>
    {#each entries as e (`${e.brush.tile}-${e.brush.contents}`)}
      <button type="button" class="sw2" role="option" aria-selected={same(e.brush, ed.block)} title={e.name} onclick={() => choose(e.brush)}>
        <Swatch info={e.info} atlas={ed.atlas} size={34} label={e.name} />
      </button>
    {/each}
  </div>
  <p class="hint">{t('edv.selected', { name: blockName })}{ed.block.layer === 'back' ? ` · ${t('edv.layerBack')}` : ''} · {t('edv.pickHint')}</p>
</section>

{#if ed.tool === 'brush'}
  <section aria-labelledby="br-h">
    <h3 id="br-h">{t('edv.brush')}</h3>
    <label class="ff">{t('edv.brushSize', { n: ed.brushSize })}<input type="range" min="1" max="16" bind:value={ed.brushSize} /></label>
    <Seg
      label={t('edv.brushShape')}
      size="sm"
      value={ed.brushRound ? 'round' : 'square'}
      options={[
        { value: 'round', label: t('edv.round') },
        { value: 'square', label: t('edv.square') }
      ]}
      onchange={(v) => (ed.brushRound = v === 'round')}
    />
  </section>
{/if}
{#if ed.tool === 'brush' || ed.tool === 'pencil'}
  <Check label={t('edv.onlySolid')} bind:checked={ed.onlySolid} />
{/if}

{#if ed.tool === 'select' || ed.tool === 'fill'}
  <section aria-labelledby="sel-h">
    <h3 id="sel-h">{t('edv.selection')}</h3>
    {#if ed.selection}
      <p class="hint">{t('edv.selSize', { w: ed.selection.w, h: ed.selection.h, n: ed.selection.w * ed.selection.h, x: ed.selection.x, y: ed.selection.y })}</p>
      <div class="acts">
        <Button size="sm" icon="copy" onclick={() => ed.copy()}>{t('edv.copy')}</Button>
        <Button size="sm" icon="cut" onclick={() => ed.copy(true)}>{t('edv.cut')}</Button>
        <Button size="sm" icon="fill" onclick={() => ed.fillSelection()}>{t('edv.fill')}</Button>
        <Button size="sm" icon="eraser" onclick={() => ed.clearSelection()}>{t('edv.clear')}</Button>
        <Button size="sm" variant="ghost" icon="x" onclick={() => (ed.selection = null)}>{t('edv.deselect')}</Button>
      </div>
      <form class="row" onsubmit={(e) => { e.preventDefault(); void ed.saveSchematic(schName || t('edv.sch.default')); schName = ''; }}>
        <input class="input" aria-label={t('edv.sch.name')} placeholder={t('edv.sch.name')} bind:value={schName} />
        <Button size="sm" type="submit" icon="save">{t('edv.sch.saveSel')}</Button>
      </form>
    {:else}
      <p class="hint">{ed.tool === 'fill' ? t('edv.fillHint') : t('edv.selectHint')}</p>
    {/if}
  </section>
{/if}

{#if ed.tool === 'paste' && ed.paste}
  <section aria-labelledby="pa-h">
    <h3 id="pa-h">{t('edv.pasteTitle')}</h3>
    <p class="hint">{t('edv.pasteHint')}</p>
    <div class="acts">
      <Button size="sm" variant="primary" icon="paste" onclick={() => ed.placePaste()}>{t('edv.pastePlace')}</Button>
      <Button size="sm" icon="rotate" onclick={() => ed.rotatePaste()}>{t('edv.rotate')}</Button>
      <Button size="sm" icon="flip" onclick={() => ed.flipPaste()}>{t('edv.flip')}</Button>
      <Button size="sm" variant="ghost" onclick={() => ed.setTool('inspect')}>{t('common.cancel')}</Button>
    </div>
    <Check label={t('edv.skipAir')} checked={ed.paste.skipAir} onchange={(v) => (ed.paste = { ...ed.paste!, skipAir: v })} />
  </section>
{/if}

{#if ed.tool === 'replace'}
  <section aria-labelledby="rp-h">
    <h3 id="rp-h">{t('edv.replace')}</h3>
    <p class="hint">{t('edv.replaceWhat', { name: brushName(cat, { layer: ed.replaceFrom.layer, tile: ed.replaceFrom.tile, contents: ed.replaceFrom.contents ?? 0 }) })}</p>
    <Button size="sm" icon="swap" onclick={() => { ed.replaceFrom = { layer: ed.block.layer, tile: ed.block.tile, contents: ed.block.contents || null }; ed.clearPreview(); }}>{t('edv.replaceSet', { name: blockName })}</Button>
    <p class="hint">{t('edv.replaceWith', { name: blockName })}</p>
    <Seg
      label={t('edv.where')}
      size="sm"
      value={ed.replaceWhole ? 'world' : 'sel'}
      options={[
        { value: 'sel', label: t('edv.whereSel') },
        { value: 'world', label: t('edv.whereWorld') }
      ]}
      onchange={(v) => { ed.replaceWhole = v === 'world'; ed.clearPreview(); }}
    />
    {#if !ed.replaceWhole && !ed.selection}<p class="hint">{t('edv.needSelection')}</p>{/if}
    {#if ed.previewCount !== null}
      <p class="hint" role="status">{t('edv.previewCount', { n: ed.previewCount })}</p>
    {/if}
    <div class="acts">
      <Button size="sm" icon="eye" disabled={!ed.replaceWhole && !ed.selection} onclick={() => ed.previewReplace()}>{t('edv.preview')}</Button>
      <Button size="sm" variant="primary" disabled={ed.previewCount === null || ed.previewCount === 0} onclick={() => ed.applyReplace()}>{t('edv.apply')}</Button>
    </div>
  </section>
{/if}

{#if ed.tool === 'select' || ed.tool === 'paste'}
  <section aria-labelledby="sch-h">
    <h3 id="sch-h">{t('edv.sch.title')}<Button size="sm" variant="ghost" icon="download" onclick={() => ed.importSchematic()}>{t('edv.sch.import')}</Button></h3>
    {#if ed.ed.clipboard}
      <Button size="sm" variant="ghost" icon="save" onclick={() => ed.saveSchematic(schName || t('edv.sch.default'), true)}>{t('edv.sch.saveClip')}</Button>
    {/if}
    {#if ed.schematics.length === 0}<p class="hint">{t('edv.sch.none')}</p>{/if}
    <ul class="list">
      {#each ed.schematics as s (s.file)}
        <li class="hrow">
          <button type="button" class="use" onclick={() => ed.useSchematic(s.file)}>
            <b>{s.name || s.file}</b>
            <span class="hint">{t('edv.sch.size', { w: s.width, h: s.height })}{s.source_world ? ` · ${t('edv.sch.from', { name: s.source_world })}` : ''}</span>
          </button>
          <Button size="sm" variant="ghost" icon="upload" label={t('edv.sch.export', { name: s.name })} onclick={() => ed.exportSchematic(s)} />
          <Button size="sm" variant="ghost" icon="trash" label={t('edv.sch.delete', { name: s.name })} onclick={() => ed.deleteSchematic(s)} />
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); display: flex; justify-content: space-between; align-items: center; gap: 8px; }
  .n { font: 600 12px var(--f-mono); }
  .pal { display: grid; grid-template-columns: repeat(5, 1fr); gap: 6px; max-height: 260px; overflow: auto; padding: 2px; }
  .sw2 { aspect-ratio: 1; border-radius: 8px; border: 2px solid var(--line-strong); padding: 0; display: grid; place-items: center; background: var(--surface-2); cursor: pointer; }
  .sw2[aria-selected='true'] { outline: 3px solid var(--focus); outline-offset: 2px; }
  .hint { font-size: 13px; color: var(--text-3); }
  .ff { display: flex; flex-direction: column; gap: 6px; font-weight: 700; font-size: 14px; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; }
  .row { display: flex; gap: 6px; }
  .row .input { flex: 1; min-width: 0; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .hrow { display: flex; align-items: center; gap: 4px; border: 2px solid var(--line); border-radius: 10px; padding: 4px 4px 4px 0; }
  .use { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; text-align: left; border: 0; background: transparent; color: inherit; padding: 4px 8px; cursor: pointer; font-size: 14px; }
</style>
