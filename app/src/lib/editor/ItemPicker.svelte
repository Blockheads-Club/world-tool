<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import { itemName, nameMatches } from './names';
  import Swatch from './Swatch.svelte';

  /** Item picker: search an item, choose a count, apply to an inventory or container slot. */
  const ed = $derived(store.editor);
  const pick = $derived(ed.itemPick!);
  let query = $state('');
  let item = $state(store.editor.itemPick!.item);
  let count = $state(String(store.editor.itemPick!.count || 1));
  const items = $derived(
    (ed.catalog?.items ?? [])
      .map((i) => ({ i, name: itemName(ed.catalog, i.id) }))
      .filter((x) => nameMatches(x.name, x.i.name, query))
      .slice(0, 200)
  );
  const n = $derived(Number(count));
  const bad = $derived(!Number.isInteger(n) || n < 0 || n > 999);

  function info(id: number) {
    const i = ed.catalog?.items.find((x) => x.id === id);
    return i?.cell ? i : i?.tile !== undefined ? ed.catalog?.tiles[i.tile] : i;
  }
  function atlasOf(id: number) {
    const i = ed.catalog?.items.find((x) => x.id === id);
    return i?.cell ? ed.itemsAtlas : ed.atlas;
  }
  function apply() {
    pick.apply(n === 0 ? 0 : item, n);
    ed.itemPick = null;
  }
</script>

<Dialog title={pick.title} icon="bag" wide onclose={() => (ed.itemPick = null)}>
  <input class="input" type="search" aria-label={t('edv.findItem')} placeholder={t('edv.findItem')} bind:value={query} data-autofocus />
  <div class="grid" role="listbox" aria-label={t('edv.items')}>
    {#each items as x (x.i.id)}
      <button type="button" class="it" role="option" aria-selected={x.i.id === item} title={x.name} onclick={() => (item = x.i.id)}>
        <Swatch info={info(x.i.id)} atlas={atlasOf(x.i.id)} size={30} label={x.name} decorative />
        <span>{x.name}</span>
      </button>
    {/each}
  </div>
  <label class="ff">{t('edv.count', { name: itemName(ed.catalog, item) })}<input class="input mono" inputmode="numeric" bind:value={count} aria-invalid={bad} /></label>
  {#if bad}<p class="err" role="alert">{t('edv.countBad')}</p>{/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (ed.itemPick = null)}>{t('common.cancel')}</Button>
    <Button variant="ghost" icon="trash" onclick={() => { pick.apply(0, 0); ed.itemPick = null; }}>{t('edv.emptyIt')}</Button>
    <Button variant="primary" disabled={bad} onclick={apply}>{t('edv.apply')}</Button>
  {/snippet}
</Dialog>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 4px; max-height: 320px; overflow: auto; margin: 10px 0; }
  .it { display: flex; gap: 8px; align-items: center; padding: 6px; border-radius: 8px; border: 2px solid transparent; background: var(--surface-2); color: inherit; cursor: pointer; text-align: left; font-size: 13px; }
  .it[aria-selected='true'] { border-color: var(--primary); background: var(--primary-soft); }
  .ff { display: flex; flex-direction: column; gap: 6px; font-weight: 700; }
  .err { color: var(--bad); font-size: 14px; }
</style>
