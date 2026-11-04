<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import { itemName } from './names';
  import Swatch from './Swatch.svelte';

  /** A grid of item slots (inventory or container); clicking a slot opens the item picker. */
  let { slots, title, onset }: { slots: [number, number][]; title: string; onset: (slot: number, item: number, count: number) => void } = $props();
  const ed = $derived(store.editor);

  function info(id: number) {
    const i = ed.catalog?.items.find((x) => x.id === id);
    return i?.cell ? i : i?.tile !== undefined ? ed.catalog?.tiles[i.tile] : i;
  }
  function open(i: number) {
    const [item, count] = slots[i];
    ed.itemPick = { title: `${title} · ${t('edv.slot', { n: i + 1 })}`, item: item || 4, count, apply: (it, n) => onset(i, it, n) };
  }
</script>

<div class="inv">
  {#each slots as [item, count], i (i)}
    {#if count > 0}
      {@const name = itemName(ed.catalog, item)}
      {@const ii = info(item)}
      <button type="button" class="it" title={name} aria-label={`${name} · ${count}`} onclick={() => open(i)}>
        <Swatch info={ii} atlas={ed.catalog?.items.find((x) => x.id === item)?.cell ? ed.itemsAtlas : ed.atlas} size={26} label={name} />
        <span>{`${count}`}</span>
      </button>
    {:else}
      <button type="button" class="it empty" aria-label={t('edv.emptySlot')} onclick={() => open(i)}></button>
    {/if}
  {/each}
</div>

<style>
  .inv { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }
  .it { aspect-ratio: 1; border-radius: 10px; background: var(--surface-2); border: 2px solid var(--line); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; font: 700 12px var(--f-mono); color: inherit; cursor: pointer; padding: 0; }
  .it:hover { border-color: var(--primary); }
</style>
