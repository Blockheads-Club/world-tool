<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import { pretty } from './data';
  import { contentsName, tileName } from './names';

  /** Search the world: chests, signs by text, any object, or the selected block type; Enter jumps to a result. */
  const ed = $derived(store.editor);
  const KINDS = ['chests', 'signs', 'objects', 'blocks'] as const;
  const blockName = $derived(ed.block.contents ? contentsName(ed.catalog, ed.block.contents) : tileName(ed.catalog, ed.block.tile));
</script>

<section aria-labelledby="sp-h">
  <h3 id="sp-h">{t('edv.searchTitle')}</h3>
  <form onsubmit={(e) => { e.preventDefault(); void ed.search(); }}>
    <input class="input" type="search" aria-label={t('edv.searchText')} placeholder={ed.searchKind === 'blocks' ? blockName : t('edv.searchText')} bind:value={ed.searchText} disabled={ed.searchKind === 'blocks'} />
  </form>
  <div class="chips" role="group" aria-label={t('edv.searchKind')}>
    {#each KINDS as k (k)}
      <button type="button" class="chip" aria-pressed={ed.searchKind === k} onclick={() => { ed.searchKind = k; void ed.search(); }}>{t(`edv.sk.${k}`)}</button>
    {/each}
  </div>
  {#if ed.results}
    <p class="hint" role="status">{t('edv.found', { n: ed.resultsTotal })}</p>
    <ol class="list">
      {#each ed.results as r, i (i)}
        <li><button type="button" class="hrow" onclick={() => { ed.goTo(r.x, r.y, false); void ed.inspect(r.x, r.y, true); }}>
          <span>{r.object ? (r.object.kind_name ? pretty(r.object.kind_name) : t('edv.unknown', { id: r.object.kind })) : blockName}{r.label ? ` · “${r.label}”` : ''}</span>
          <span class="mono hint">{`X ${r.x} · Y ${r.y}`}</span>
        </button></li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { min-height: 32px; padding: 0 12px; border-radius: 999px; border: 2px solid var(--line); background: var(--surface); color: var(--text-2); font: 700 13px var(--f-body); cursor: pointer; }
  .chip[aria-pressed='true'] { background: var(--text); border-color: var(--text); color: var(--bg); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; max-height: 420px; overflow: auto; }
  .hrow { display: flex; gap: 10px; justify-content: space-between; align-items: center; padding: 8px 10px; border-radius: 10px; font-size: 14px; width: 100%; border: 0; background: transparent; color: inherit; cursor: pointer; text-align: left; }
  .hrow:hover { background: var(--surface-2); }
  .hint { font-size: 13px; color: var(--text-3); }
</style>
