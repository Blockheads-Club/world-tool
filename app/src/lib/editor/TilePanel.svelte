<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import { toast } from '../ui/toast.svelte';
  import { pretty } from './data';
  import { contentsName, tileName } from './names';
  import Slots from './Slots.svelte';
  import Swatch from './Swatch.svelte';

  /** Inspector: what is at the selected tile. */
  const ed = $derived(store.editor);
  const tile = $derived(ed.tile);
  const cat = $derived(ed.catalog);

  async function copy() {
    await navigator.clipboard.writeText(`${tile!.x} ${tile!.y}`);
    toast(t('edv.copied'));
  }
</script>

<section aria-labelledby="tp-h">
  <h3 id="tp-h">{t('edv.panel.tile')}</h3>
  {#if !tile}
    <p class="hint">{t('edv.tileHint')}</p>
  {:else}
    <div class="head">
      <Swatch info={tile.stored ? cat!.tiles[tile.fg] : undefined} atlas={ed.atlas} size={40} label={tileName(cat, tile.fg)} />
      <div>
        <b>{tile.stored ? tileName(cat, tile.fg) : t('edv.notGenerated')}</b>
        <span class="mono hint">{`X ${tile.x} · Y ${tile.y}`}</span>
      </div>
    </div>
    {#if tile.stored}
      <dl class="kv">
        <dt>{t('edv.background')}</dt><dd>{tileName(cat, tile.bg)}</dd>
        <dt>{t('edv.contents')}</dt><dd>{contentsName(cat, tile.contents)}</dd>
        <dt>{t('edv.light')}</dt><dd>{t('edv.lightVal', { n: Math.max(tile.light, tile.sun_light) })}</dd>
      </dl>
      {#if tile.protected}<p class="hint">{t('edv.protected')}</p>{/if}
      {#if tile.objects.length}
        <h3>{t('edv.objects')}</h3>
        <ul class="objs">
          {#each tile.objects as o, i (i)}
            <li>
              <b>{o.kind_name ? pretty(o.kind_name) : t('edv.unknown', { id: o.kind })}</b> <span class="mono hint">{`X ${o.x} · Y ${o.y}`}</span>{#if o.text}<span class="hint">{`“${o.text}”`}</span>{/if}
              {#if o.slots}
                <span class="hint">{t('edv.contentsSlots', { n: o.slots.filter((s) => s[1] > 0).length, of: o.slots.length })}</span>
                <Slots slots={o.slots} title={pretty(o.kind_name ?? '')} onset={(slot, item, count) => ed.edit({ op: 'set_container_stack', key: o.key, index: o.index, slot, item, count })} />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
    <div class="kpi"><Button size="sm" variant="ghost" icon="copy" onclick={copy}>{t('edv.copyCoords')}</Button></div>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .head { display: flex; gap: 12px; align-items: center; }
  .head div { display: flex; flex-direction: column; }
  .head b { font: 800 17px var(--f-display); }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; margin: 0; font-size: 14px; }
  .kv dt { color: var(--text-2); }
  .kv dd { margin: 0; }
  .objs { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; font-size: 14px; }
  .objs li { display: flex; flex-wrap: wrap; gap: 6px; align-items: baseline; }
  .objs li :global(.inv) { width: 100%; }
  .hint { font-size: 13.5px; color: var(--text-3); }
  .kpi { display: flex; gap: 10px; flex-wrap: wrap; }
</style>
