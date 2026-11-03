<script lang="ts">
  import { bytes } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import type { Choice } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Seg from '../ui/Seg.svelte';

  /** ImportDialog: browse or drop `.bhworld` files and server folders; keep both / replace / skip per conflict. */
  const f = store.importer;
  const n = $derived(f.importable.length);
</script>

<Dialog title={t('imp.title')} icon="download" wide busy={f.busy} onclose={() => f.close()}>
  <div class="drop">
    <img src="/bhworld-file.svg" alt="" width="40" height="40" />
    <div>
      <b>{t('imp.drop')}</b><br />
      <span class="hint">{t('imp.or')}</span>
      <button class="link" onclick={() => void f.browse(false)}>{t('imp.browseFiles')}</button> ·
      <button class="link" onclick={() => void f.browse(true)}>{t('imp.chooseFolder')}</button>
    </div>
  </div>
  {#if f.items.length}
    <table class="tbl" aria-label={t('imp.items')}>
      <tbody>
        {#each f.items as i (i.path)}
          <tr class={[i.status === 'conflict' && 'rsel']}>
            <td class="ic"><Icon name={i.kind === 'server' ? 'server' : 'file'} /></td>
            <td>
              <b>{i.name}</b><br />
              <span class="hint">
                {#if i.status === 'invalid'}{t('imp.notWorld')}
                {:else if i.status === 'conflict'}{i.conflict === 'same_id' ? t('imp.sameId', { name: i.existing ?? '' }) : t('imp.sameName', { name: i.existing ?? '' })}
                {:else if i.kind === 'server'}{t('imp.serverFolder', { size: bytes(i.size), v: i.game_version ?? '-' })}
                {:else}{t('imp.fromFile', { size: bytes(i.size), v: i.game_version ?? '-' })}{/if}
              </span>
            </td>
            <td class="r">
              {#if i.status === 'invalid'}<Badge tone="bad" icon="x">{t('imp.skipped')}</Badge>
              {:else if i.status === 'new'}<Badge tone="ok" icon="plus">{t('imp.new')}</Badge>
              {:else}
                <Seg size="sm" label={t('tr.whatToDo', { name: i.name })} value={f.choices[i.path]} onchange={(v: Choice) => (f.choices[i.path] = v)}
                  options={[{ value: 'keep_both', label: t('choice.keep_both') }, { value: 'replace', label: t('choice.replace') }, { value: 'skip', label: t('choice.skip') }]} />
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if f.items.some((i) => i.status === 'conflict')}<p class="hint">{t('imp.hint')}</p>{/if}
  {/if}
  {#snippet footer()}
    <Button onclick={() => f.close()} disabled={f.busy}>{t('common.cancel')}</Button>
    <Button variant="primary" icon="download" loading={f.busy} disabled={n === 0} onclick={() => void f.run()}>{t('imp.run', { n })}</Button>
  {/snippet}
</Dialog>

<style>
  .drop { grid-auto-flow: column; gap: 14px; }
  .ic { width: 36px; color: var(--text-2); }
  .r { text-align: right; }
</style>
