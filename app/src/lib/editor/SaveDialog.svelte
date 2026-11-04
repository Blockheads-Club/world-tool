<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import { stepText } from './history';

  /** "Save changes to …?" when leaving with unsaved changes. */
  const ed = $derived(store.editor);
  const steps = $derived(ed.ed.history.slice(0, ed.ed.cursor).slice(-8));
</script>

<Dialog title={t('edv.saveAsk', { name: ed.ed.name ?? ed.world?.name ?? '' })} icon="save" alert busy={ed.saving} onclose={() => (ed.saveAsk = false)}>
  <p>{t('edv.saveAskText', { n: ed.ed.unsaved })}</p>
  <ul class="ol">
    {#each steps as l, i (i)}<li>{stepText(ed.catalog, l)}</li>{/each}
  </ul>
  <p class="hint">{t('edv.saveAskHint')}</p>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (ed.saveAsk = false)}>{t('common.cancel')}</Button>
    <Button variant="quiet-bad" onclick={() => ed.discard()}>{t('edv.dontSave')}</Button>
    <Button loading={ed.saving} onclick={() => ed.save(true)}>{t('edv.saveNew')}</Button>
    <Button variant="primary" loading={ed.saving} autofocus onclick={() => ed.save(false)}>{t('edv.saveWorlds')}</Button>
  {/snippet}
</Dialog>

<style>
  .ol { margin: 10px 0; padding-left: 20px; font-size: 14px; }
  .hint { font-size: 13px; color: var(--text-3); }
</style>
