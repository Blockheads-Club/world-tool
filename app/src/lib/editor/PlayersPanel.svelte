<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Slots from './Slots.svelte';

  /** Players: the blockheads saved in the world — needs, inventory, move, remove. */
  const ed = $derived(store.editor);
  const b = $derived(ed.picked === null ? null : ed.blockheads[ed.picked]);
  const STATS = ['edv.health', 'edv.happiness', 'edv.food', 'edv.energy'] as const;

  function setStat(i: number, v: string) {
    const bh = b!;
    const pct = Number(v);
    if (!Number.isFinite(pct)) return;
    const stats = [...(bh.stats as [number, number, number, number])] as [number, number, number, number];
    stats[i] = Math.max(0, Math.min(100, pct)) / 100;
    void ed.edit({ op: 'set_stats', player: bh.player, id: bh.id, stats });
  }
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
      <div class="stats">
        {#each STATS as k, i (k)}
          <label>{t(k)}<input class="input mono" type="number" min="0" max="100" value={Math.round(b.stats[i] * 100)} onchange={(e) => setStat(i, (e.currentTarget as HTMLInputElement).value)} /></label>
        {/each}
      </div>
    {/if}
    <h3>{t('edv.inventory')}</h3>
    {#if b.inventory}
      <Slots slots={b.inventory} title={b.name} onset={(slot, item, count) => ed.edit({ op: 'set_stack', player: b.player, id: b.id, slot, item, count })} />
    {:else}
      <p class="hint">{t('edv.noInventory')}</p>
    {/if}
    <div class="acts">
      <Button size="sm" icon="target" onclick={() => ed.goTo(b.x, b.y, false)}>{t('edv.show')}</Button>
      <Button size="sm" icon="portal" onclick={() => (ed.teleport = ed.picked)}>{t('edv.teleport')}</Button>
      <Button size="sm" variant="quiet-bad" icon="trash" onclick={() => (ed.removeAsk = ed.picked)}>{t('edv.removePlayer')}</Button>
    </div>
  {/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .hrow { display: flex; gap: 10px; align-items: center; justify-content: space-between; padding: 8px 10px; border-radius: 10px; font-size: 14px; width: 100%; border: 0; background: transparent; color: inherit; cursor: pointer; text-align: left; }
  .hrow.cur { background: var(--primary-soft); color: var(--primary-ink); }
  .stats { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .stats label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 700; color: var(--text-2); }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; }
  .hint { font-size: 13.5px; color: var(--text-3); }
</style>
