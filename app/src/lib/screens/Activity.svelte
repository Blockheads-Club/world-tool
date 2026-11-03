<script lang="ts">
  import { bytes, when } from '../format';
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import type { Key } from '../messages';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { Activity } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Empty from '../ui/Empty.svelte';
  import Icon from '../ui/Icon.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';

  type Filter = 'all' | 'reads' | 'copies' | 'backups' | 'errors';
  let list = $state<Activity[] | null>(null);
  let filter = $state<Filter>('all');
  let clearing = $state(false);

  async function load() {
    list = await store.api.listActivity();
  }
  $effect(() => {
    void load();
  });

  const MATCH: Record<Filter, (a: Activity) => boolean> = {
    all: () => true,
    reads: (a) => a.kind === 'read' || a.kind === 'export',
    copies: (a) => a.kind === 'copy' || a.kind === 'import',
    backups: (a) => a.kind === 'backup' || a.kind === 'restore',
    errors: (a) => a.result !== 'done'
  };
  const shown = $derived((list ?? []).filter(MATCH[filter]));
  const ICON: Record<Activity['kind'], IconName> = { read: 'download', copy: 'upload', backup: 'shield', restore: 'history', import: 'download', export: 'file' };

  function what(a: Activity): string {
    const n = a.worlds.length;
    const key = `act.${a.kind}.${a.result === 'done' ? 'done' : 'failed'}` as Key;
    return t(key, { n, device: a.device });
  }
  function line(a: Activity): string {
    if (a.result !== 'done') return t(`err.${a.error ?? 'io'}` as Key);
    const parts = [a.worlds.join(', '), a.size ? bytes(a.size) : ''].filter(Boolean);
    return parts.join(' · ');
  }

  async function copyLog() {
    const text = (list ?? []).map((a) => `${new Date(a.time).toISOString()}\t${a.result}\t${what(a)}\t${line(a)}${a.detail ? `\t${a.detail}` : ''}`).join('\n');
    toast((await copyText(text)) ? t('common.copied') : t('common.copyFailed'));
  }

  async function clear() {
    clearing = false;
    await store.api.clearActivity();
    await load();
  }
</script>

<Header title={t('nav.activity')} sub={t('act.sub')} />
<div class="main">
  {#if list && list.length === 0}
    <Empty icon="history" title={t('act.emptyTitle')} text={t('act.emptyText')} />
  {:else if list}
    <div class="tools">
      <div class="chips" role="group" aria-label={t('act.filter')}>
        {#each ['all', 'reads', 'copies', 'backups', 'errors'] as const as k (k)}
          <button class="chip" aria-pressed={filter === k} onclick={() => (filter = k)}>{t(`act.f.${k}`)}</button>
        {/each}
      </div>
      <span class="sp"></span>
      <Button size="sm" variant="ghost" icon="copy" onclick={() => void copyLog()}>{t('act.copy')}</Button>
      <Button size="sm" variant="ghost" icon="trash" onclick={() => (clearing = true)}>{t('act.clear')}</Button>
    </div>
    <div class="card scroll">
      <table class="tbl" aria-label={t('nav.activity')}>
        <thead><tr><th>{t('act.when')}</th><th><span class="sr">{t('act.kind')}</span></th><th>{t('act.what')}</th><th>{t('tr.result')}</th></tr></thead>
        <tbody>
          {#each shown as a (a.id)}
            <tr>
              <td class="hint nw">{when(a.time, Date.now(), true)}</td>
              <td><Icon name={ICON[a.kind]} /></td>
              <td>
                <b>{what(a)}</b><br /><span class="muted">{line(a)}</span>
                {#if a.detail}<details><summary class="hint">{t('act.details')}</summary><pre class="log"><span class="e">{a.detail}</span></pre></details>{/if}
              </td>
              <td>
                {#if a.result === 'done'}<Badge tone="ok" icon="check">{t('act.r.done')}</Badge>
                {:else if a.result === 'failed'}<Badge tone="bad" icon="x">{t('act.r.failed')}</Badge>
                {:else}<Badge tone="warn" icon="alert">{t('act.r.stopped')}</Badge>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if shown.length === 0}<p class="muted none">{t('act.noneHere')}</p>{/if}
    </div>
  {/if}
</div>
{#if clearing}
  <Dialog title={t('act.clearTitle')} icon="trash" tone="bad" alert onclose={() => (clearing = false)}>
    <p>{t('act.clearText')}</p>
    {#snippet footer()}
      <Button onclick={() => (clearing = false)} autofocus>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" onclick={() => void clear()}>{t('act.clear')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .tools { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .sp { flex: 1; }
  .scroll { overflow-x: auto; }
  .nw { white-space: nowrap; }
  details { margin-top: 6px; }
  summary { cursor: pointer; }
  .none { padding: 16px; }
</style>
