<script lang="ts">
  import { day } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Toggle from '../ui/Toggle.svelte';

  /** World settings: name and time of day can be changed; the rest is shown. Editing options live here too. */
  const ed = $derived(store.editor);
  const w = $derived(ed.world!);
  const two = (n: number) => String(Math.floor(n)).padStart(2, '0');
  const hour = $derived(ed.ed.time_of_day);
  let name = $state(store.editor.ed.name ?? '');

  function rename() {
    if (name.trim() && name.trim().toUpperCase() !== ed.ed.name) void ed.edit({ op: 'set_name', name });
  }
</script>

<section aria-labelledby="wp-h">
  <h3 id="wp-h">{t('edv.panel.world')}</h3>
  <label class="ff">{t('edv.world.name')}
    <input class="input" bind:value={name} maxlength="64" onchange={rename} />
  </label>
  {#if hour !== null}
    <label class="ff">{t('edv.world.timeVal', { time: `${two(hour)}:${two((hour % 1) * 60)}` })}
      <input type="range" min="0" max="23.5" step="0.5" value={hour} onchange={(e) => ed.edit({ op: 'set_time_of_day', hour: Number((e.currentTarget as HTMLInputElement).value) })} />
    </label>
  {/if}
  <dl class="kv">
    <dt>{t('edv.world.mode')}</dt><dd>{w.expert ? t('edv.world.expert') : t('edv.world.normal')}</dd>
    <dt>{t('edv.world.size')}</dt><dd>{t('edv.world.sizeVal', { w: w.width, h: w.height })}</dd>
    {#if w.game_version}<dt>{t('edv.world.version')}</dt><dd>{`${w.game_version}`}</dd>{/if}
    {#if w.created}<dt>{t('edv.world.created')}</dt><dd>{day(w.created * 1000)}</dd>{/if}
    <dt>{t('edv.world.id')}</dt><dd class="mono id">{`${w.world_id}`}</dd>
  </dl>
  <p class="hint">{t('edv.world.fixed')}</p>
  {#if w.server}<p class="hint">{t('edv.world.server')}</p>{/if}
  <h3>{t('edv.opts')}</h3>
  <Toggle label={t('edv.optObjects')} checked={store.settings!.editor_objects} onchange={(v) => store.saveSettings({ editor_objects: v })} />
  <p class="hint">{t('edv.optObjectsText')}</p>
  <Toggle label={t('edv.optGenerate')} checked={store.settings!.editor_generate} onchange={(v) => store.saveSettings({ editor_generate: v })} />
  <p class="hint">{t('edv.optGenerateText')}</p>
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { font-size: 15px; color: var(--text-2); }
  .ff { display: flex; flex-direction: column; gap: 6px; font-weight: 700; font-size: 14px; }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 14px; margin: 0; font-size: 14px; }
  .kv dt { color: var(--text-2); }
  .kv dd { margin: 0; min-width: 0; }
  .id { overflow-wrap: anywhere; font-size: 12.5px; }
  .hint { font-size: 13px; color: var(--text-3); }
</style>
