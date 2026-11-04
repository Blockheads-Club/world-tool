<script lang="ts">
  import { day } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';

  /** World settings (read-only in the viewer). */
  const w = $derived(store.editor.world!);
  const two = (n: number) => String(Math.floor(n)).padStart(2, '0');
</script>

<section aria-labelledby="wp-h">
  <h3 id="wp-h">{t('edv.panel.world')}</h3>
  <dl class="kv">
    <dt>{t('edv.world.name')}</dt><dd>{`${w.name}`}</dd>
    <dt>{t('edv.world.mode')}</dt><dd>{w.expert ? t('edv.world.expert') : t('edv.world.normal')}</dd>
    <dt>{t('edv.world.size')}</dt><dd>{t('edv.world.sizeVal', { w: w.width, h: w.height })}</dd>
    {#if w.time_of_day !== null}<dt>{t('edv.world.time')}</dt><dd class="mono">{`${two(w.time_of_day)}:${two((w.time_of_day % 1) * 60)}`}</dd>{/if}
    {#if w.game_version}<dt>{t('edv.world.version')}</dt><dd>{`${w.game_version}`}</dd>{/if}
    {#if w.created}<dt>{t('edv.world.created')}</dt><dd>{day(w.created * 1000)}</dd>{/if}
    <dt>{t('edv.world.id')}</dt><dd class="mono id">{`${w.world_id}`}</dd>
  </dl>
  {#if w.server}<p class="hint">{t('edv.world.server')}</p>{/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; margin: 0; font-size: 14px; }
  .kv dt { color: var(--text-2); }
  .kv dd { margin: 0; min-width: 0; }
  .id { overflow-wrap: anywhere; font-size: 12.5px; }
  .hint { font-size: 13.5px; color: var(--text-3); }
</style>
