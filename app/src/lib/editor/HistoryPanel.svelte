<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import { stepText } from './history';

  /** History: every step, the current one highlighted, undone ones dimmed; click a step to go back to it. */
  const ed = $derived(store.editor);
</script>

<section aria-labelledby="hp-h">
  <h3 id="hp-h">{t('edv.panel.history')}</h3>
  <ol class="list">
    <li><button type="button" class={['hrow', ed.ed.cursor === 0 && 'cur']} aria-current={ed.ed.cursor === 0 || undefined} onclick={() => ed.history(0)}>{t('edv.openCopy')}</button></li>
    {#each ed.ed.history as l, i (i)}
      <li>
        <button type="button" class={['hrow', i + 1 === ed.ed.cursor && 'cur', i + 1 > ed.ed.cursor && 'fut']} aria-current={i + 1 === ed.ed.cursor || undefined} onclick={() => ed.history(i + 1)}>
          {stepText(ed.catalog, l)}{#if i + 1 > ed.ed.cursor}<span class="hint">{t('edv.undone')}</span>{/if}
        </button>
      </li>
    {/each}
  </ol>
  <p class="hint">{t('edv.historyHint')}</p>
  <Button size="sm" icon="compare" onclick={() => ed.showCompare()}>{ed.compare ? t('edv.compareOff') : t('edv.compare')}</Button>
  {#if ed.compare}<p class="hint" role="status">{t('edv.compareCount', { n: ed.compare.length })}</p>{/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .hrow { display: flex; gap: 10px; justify-content: space-between; align-items: center; padding: 8px 10px; border-radius: 10px; font-size: 14px; width: 100%; border: 0; background: transparent; color: inherit; cursor: pointer; text-align: left; }
  .hrow.cur { background: var(--primary-soft); color: var(--primary-ink); font-weight: 700; }
  .hrow.fut { opacity: 0.55; }
  .hint { font-size: 13px; color: var(--text-3); }
</style>
