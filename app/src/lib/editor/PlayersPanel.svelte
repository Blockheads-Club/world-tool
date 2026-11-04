<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import { itemName } from './names';
  import Swatch from './Swatch.svelte';

  /** Players: the blockheads saved in the world, their needs and inventory (read-only for now). */
  const ed = $derived(store.editor);
  const b = $derived(ed.picked === null ? null : ed.blockheads[ed.picked]);
  const STATS = ['edv.health', 'edv.happiness', 'edv.food', 'edv.energy'] as const;
</script>

<section aria-labelledby="pp-h">
  <h3 id="pp-h">{t('edv.panel.players')} · {t('edv.blockheads', { n: ed.blockheads.length })}</h3>
  {#if ed.blockheads.length === 0}
    <p class="hint">{t('edv.noPlayers')}</p>
  {/if}
  <ul class="list">
    {#each ed.blockheads as bh, i (bh.player + bh.id)}
      <li><button type="button" class={['hrow', i === ed.picked && 'cur']} aria-pressed={i === ed.picked} onclick={() => ed.pick(i)}>
        <b>{bh.name}</b><span class="mono hint">{`X ${bh.x} · Y ${bh.y}`}</span>
      </button></li>
    {/each}
  </ul>
  {#if b}
    <h3>{b.name}</h3>
    {#if b.stats}
      <dl class="kv">
        {#each STATS as k, i (k)}<dt>{t(k)}</dt><dd>{`${Math.round(b.stats[i] * 100)}%`}</dd>{/each}
      </dl>
    {/if}
    <h3>{t('edv.inventory')}</h3>
    {#if b.inventory}
      <div class="inv">
        {#each b.inventory as [item, count], i (i)}
          {#if count > 0}
            {@const name = itemName(ed.catalog, item)}
            <div class="it" title={name}>
              <Swatch info={ed.catalog?.items.find((x) => x.id === item)} atlas={ed.itemsAtlas} size={26} label={name} />
              <span>{`${count}`}</span>
            </div>
          {:else}
            <div class="it empty"><span class="sr">{t('edv.emptySlot')}</span></div>
          {/if}
        {/each}
      </div>
    {:else}
      <p class="hint">{t('edv.noInventory')}</p>
    {/if}
    <div><Button size="sm" icon="target" onclick={() => ed.goTo(b.x, b.y, false)}>{t('edv.show')}</Button></div>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .hrow { display: flex; gap: 10px; align-items: center; justify-content: space-between; padding: 8px 10px; border-radius: 10px; font-size: 14px; width: 100%; border: 0; background: transparent; color: inherit; cursor: pointer; text-align: left; }
  .hrow.cur { background: var(--primary-soft); color: var(--primary-ink); }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; margin: 0; font-size: 14px; }
  .kv dt { color: var(--text-2); }
  .kv dd { margin: 0; font-family: var(--f-mono); }
  .inv { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
  .it { aspect-ratio: 1; border-radius: 10px; background: var(--surface-2); border: 2px solid var(--line); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; font: 700 12px var(--f-mono); }
  .hint { font-size: 13.5px; color: var(--text-3); }
</style>
