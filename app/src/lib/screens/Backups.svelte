<script lang="ts">
  import { bytes, clock, dash, pct, when, zero } from '../format';
  import { t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { Backup, Progress as P } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Check from '../ui/Check.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Empty from '../ui/Empty.svelte';
  import Menu from '../ui/Menu.svelte';
  import Notice from '../ui/Notice.svelte';
  import PhoneWait from '../ui/PhoneWait.svelte';
  import Progress from '../ui/Progress.svelte';
  import { toast } from '../ui/toast.svelte';
  import { opId } from '../util';

  let list = $state<Backup[] | null>(null);
  let device = $state<string | null>(null);
  let restoring = $state<Backup | null>(null);
  let target = $state('');
  let closed = $state(false);
  let running = $state<P | null | 'start'>(null);
  let deleting = $state<Backup | null>(null);
  let op = '';
  const folder = $derived(`${store.settings!.worlds_folder}/Backups`);

  async function load() {
    list = await store.api.listBackups();
    if (store.restoreId) {
      open(list.find((b) => b.id === store.restoreId) ?? null);
      store.restoreId = null;
    }
  }
  $effect(() => {
    void load();
  });

  const devices = $derived([...new Map(list!.map((b) => [b.device_id, b.device])).entries()]);
  const shown = $derived(list!.filter((b) => !device || b.device_id === device));
  const total = $derived(list!.reduce((s, b) => s + b.size, 0));
  const targets = $derived(store.devices.filter((d) => d.kind !== 'ios' && d.status === 'ready'));

  function open(b: Backup | null) {
    if (!b) return;
    restoring = b;
    closed = false;
    target = targets.find((d) => d.id === b.device_id)?.id ?? targets[0]?.id ?? '';
  }

  function reason(b: Backup): string {
    if (b.reason === 'before_write') return t('bk.beforeWrite', { n: zero(b.reason_count) });
    if (b.reason === 'read') return t('bk.read');
    return t('bk.beforeRestore');
  }

  async function restore() {
    const b = restoring as Backup;
    op = opId('restore');
    running = 'start';
    try {
      await store.api.restoreBackup(op, b.id, target, (p) => (running = p));
      toast(t('bk.restored', { name: store.devices.find((d) => d.id === target)?.name ?? '' }));
      restoring = null;
    } catch (e) {
      store.fail(e, () => void restore());
    } finally {
      running = null;
      await load();
    }
  }

  async function remove() {
    const b = deleting as Backup;
    deleting = null;
    await store.api.deleteBackup(b.id);
    await load();
  }

  const isAndroid = $derived(store.devices.find((d) => d.id === target)?.kind === 'android');
  const viaRoot = $derived(!!store.devices.find((d) => d.id === target)?.root && store.settings?.root_mode !== 'never');
  const prog = $derived(running && running !== 'start' ? running : null);
  const waiting = $derived(prog?.stage === 'phone_backup' || prog?.stage === 'phone_restore' || prog?.stage === 'phone_check');
</script>

<Header title={t('nav.backups')} sub={list && list.length ? t('bk.sub', { n: list.length, size: bytes(total) }) : t('bk.subEmpty')}>
  {#snippet actions()}<Button variant="ghost" icon="folder" onclick={() => void store.api.showInFolder(folder)}>{t('worlds.openFolder')}</Button>{/snippet}
</Header>
<div class="main">
  {#if list && list.length === 0}
    <Empty icon="shield" title={t('bk.emptyTitle')} text={t('bk.emptyText')} />
  {:else if list}
    <div class="chips" role="group" aria-label={t('bk.filter')}>
      <button class="chip" aria-pressed={device === null} onclick={() => (device = null)}>{t('bk.allDevices')} <span class="n">{list.length}</span></button>
      {#each devices as [id, name] (id)}
        <button class="chip" aria-pressed={device === id} onclick={() => (device = id)}>{`${name} `}<span class="n">{list.filter((b) => b.device_id === id).length}</span></button>
      {/each}
    </div>
    <div class="card scroll">
      <table class="tbl">
        <thead><tr><th>{t('bk.device')}</th><th>{t('bk.saved')}</th><th>{t('bk.contains')}</th><th>{t('read.size')}</th><th>{t('bk.kind')}</th><th><span class="sr">{t('bk.actions')}</span></th></tr></thead>
        <tbody>
          {#each shown as b, i (b.id)}
            <tr class={[i === 0 && 'rsel']}>
              <td><b>{b.device}</b><br /><span class="hint">{reason(b)}</span></td>
              <td>{when(b.time)}</td>
              <td>{t('bk.worlds', { n: b.worlds })}</td>
              <td class="mono">{bytes(b.size)}</td>
              <td>{#if b.automatic}<Badge tone="info" icon="shield">{t('bk.auto')}</Badge>{:else}<Badge>{t('bk.manual')}</Badge>{/if}</td>
              <td class="acts">
                <Button size="sm" variant={i === 0 ? 'primary' : 'default'} icon="history" onclick={() => open(b)}>{t('bk.restore')}</Button>
                <Menu label={t('bk.more', { name: b.device })} items={[
                  { label: t('common.showInFolder'), icon: 'folder', run: () => void store.api.showInFolder(folder) },
                  { label: t('common.delete'), icon: 'trash', danger: true, run: () => (deleting = b) }
                ]} />
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="hint">{t('bk.keep', { n: store.settings!.backups_to_keep })}</p>
  {/if}
</div>

{#if restoring}
  {@const b = restoring}
  <Dialog title={t('bk.restoreTitle')} icon="history" busy={running !== null} wide={waiting} onclose={() => (restoring = null)}>
    {#if prog && waiting}
      <PhoneWait screen={prog.stage === 'phone_restore' ? 'restore' : 'backup'} step={prog.stage === 'phone_backup' ? 1 : prog.stage === 'phone_restore' ? 2 : 3} elapsed={zero(prog.elapsed)}
        title={prog.stage === 'phone_backup' ? t('tr.phoneBackupTitle') : prog.stage === 'phone_check' ? t('tr.phoneCheckTitle') : t('tr.phoneRestoreTitle')}
        text={prog.stage === 'phone_backup' ? t('bk.phoneBackupText') : prog.stage === 'phone_check' ? t('tr.phoneCheckText') : t('tr.phoneRestoreText')}>
        {#snippet actions()}<Button icon="x" onclick={() => void store.api.cancel(op)}>{t('common.cancel')}</Button>{/snippet}
      </PhoneWait>
    {:else if running}
      <Progress title={t('bk.restoring')} value={pct(prog?.done, prog?.total)} />
    {:else}
      <dl class="kv"><dt>{t('bk.backup')}</dt><dd>{`${b.device} · ${when(b.time)} · ${bytes(b.size)}`}</dd></dl>
      <div class="ffield">
        <label for="rt">{t('bk.restoreTo')}</label>
        <select id="rt" class="sel" onchange={(e) => (target = targets[e.currentTarget.selectedIndex].id)}>
          {#each targets as d (d.id)}<option selected={d.id === target}>{`${d.name}${d.kind === 'android' ? ` · ${d.connection === 'wifi' ? 'Wi-Fi' : 'USB'}` : ''} · ${t('read.gameV', { v: dash(d.game_version) })}`}</option>{/each}
        </select>
        {#if targets.length === 0}<span class="hint">{t('bk.noTarget')}</span>{/if}
      </div>
      <Notice tone="bad" role="alert" title={t('bk.replaces')}>{t('bk.replacesText', { time: clock(b.time) })}</Notice>
      {#if isAndroid && !viaRoot}<p class="hint">{t('tr.threeTaps')}</p>{/if}
      {#if isAndroid}<Check bind:checked={closed} label={t('bk.closed')} />{/if}
    {/if}
    {#snippet footer()}
      {#if !running}
        <Button onclick={() => (restoring = null)}>{t('common.cancel')}</Button>
        <Button variant="primary" icon="history" disabled={!target || (isAndroid && !closed)} onclick={() => void restore()}>{t('bk.restoreBtn')}</Button>
      {/if}
    {/snippet}
  </Dialog>
{/if}
{#if deleting}
  <Dialog title={t('bk.deleteTitle')} icon="trash" tone="bad" alert onclose={() => (deleting = null)}>
    <p>{t('bk.deleteText', { name: deleting.device, when: when(deleting.time) })}</p>
    {#snippet footer()}
      <Button onclick={() => (deleting = null)} autofocus>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" onclick={() => void remove()}>{t('common.delete')}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .scroll { overflow-x: auto; }
  .acts { white-space: nowrap; text-align: right; }
  .acts > :global(*) { vertical-align: middle; }
</style>
