<script lang="ts">
  import { blank, dash, day, zero } from '../format';
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import type { Device } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Menu from '../ui/Menu.svelte';

  /** DeviceRow + StatusBadge: name, model, connection, game version, root; actions by status. */
  let { device: d, onforget, ondiag }: { device: Device; onforget: () => void; ondiag: () => void } = $props();

  const icon = $derived(d.kind === 'pc' ? 'laptop' : d.kind === 'ios' ? 'archive' : 'phone');
  const ago = $derived(Math.max(1, Math.round((Date.now() - zero(d.last_seen)) / 86_400_000)));
  const model = $derived(
    d.kind === 'pc' ? blank(d.path) : d.kind === 'ios' ? t('dev.iosModel', { model: d.model, v: dash(d.os_version), date: day(zero(d.backup_date)) }) : t('dev.androidModel', { model: d.model, v: dash(d.os_version) })
  );
  const guide = $derived(d.kind === 'ios' ? 'ios' : d.connection === 'wifi' ? 'wifi' : 'usb');
  const more = $derived([
    { label: t('dev.guide'), icon: 'info' as const, run: () => store.openGuide(guide) },
    ...(d.connection === 'wifi' ? [{ label: t('dev.forget'), icon: 'trash' as const, danger: true, run: onforget }] : []),
    { label: t('common.copyDiag'), icon: 'copy' as const, run: ondiag }
  ]);
</script>

<div class={['dev', d.status === 'unauthorized' && 'att']}>
  <span class="di"><Icon name={icon} /></span>
  <div class="dm">
    <span class="nm">
      <b>{d.name}</b>
      {#if d.read_only}<Badge tone="info" icon="eye">{t('dev.readOnly')}</Badge>
      {:else if d.status === 'ready'}<Badge tone="ok" dot>{t('dev.ready')}</Badge>
      {:else if d.status === 'unauthorized'}<Badge tone="warn" icon="lock">{t('dev.unauth')}</Badge>
      {:else}<Badge icon="wifioff">{t('dev.offline')}</Badge>{/if}
    </span>
    {#if model}<span class="hint path">{model}</span>{/if}
    <span class="facts">
      {#if d.connection === 'usb'}<span><Icon name="usb" size={16} />USB</span>{/if}
      {#if d.connection === 'wifi'}<span><Icon name="wifi" size={16} />{d.address && d.status === 'ready' ? `Wi-Fi · ${d.address}` : 'Wi-Fi'}</span>{/if}
      {#if d.encrypted}<span><Icon name="lock" size={16} />{t('dev.encrypted')}</span>{/if}
      {#if d.status === 'unauthorized'}
        <span class="warn"><Icon name="alert" size={16} />{t('dev.tapAllow')}</span>
      {:else if d.status === 'offline'}
        {#if d.last_seen}<span><Icon name="clock" size={16} />{t('dev.lastSeen', { n: ago })}</span>{/if}
      {:else}
        {#if d.game_version}<span><Icon name="box" size={16} />{t('dev.game', { v: d.game_version })}</span>{:else}<span class="warn"><Icon name="alert" size={16} />{t('dev.noGame')}</span>{/if}
        {#if d.kind === 'android'}<span><Icon name={d.root ? 'root' : 'shield'} size={16} />{d.root ? t('dev.root') : t('dev.noRoot')}</span>{/if}
        {#if d.kind === 'pc'}<span><Icon name="check" size={16} />{t('dev.found', { n: zero(d.world_count) })}</span>{/if}
      {/if}
    </span>
  </div>
  <div class="da">
    {#if d.status === 'unauthorized'}
      <Button size="sm" icon="info" onclick={() => (store.shown = { error: { code: 'not_allowed', device: d.name }, retry: () => void store.refreshDevices() })}>{t('dev.howAllow')}</Button>
      <Button size="sm" variant="ghost" icon="refresh" onclick={() => void store.refreshDevices()}>{t('common.refresh')}</Button>
    {:else if d.status === 'offline'}
      <Button size="sm" icon="refresh" onclick={() => void store.refreshDevices()}>{t('dev.reconnect')}</Button>
    {:else if d.kind === 'ios'}
      <Button size="sm" variant="primary" icon="download" onclick={() => store.read.start(d)}>{t('dev.export')}</Button>
    {:else}
      <Button size="sm" variant="primary" icon="download" onclick={() => store.read.start(d)}>{t('dev.read')}</Button>
      <Button size="sm" icon="upload" onclick={() => store.transfer.start({ to: d.id })}>{t('dev.write')}</Button>
    {/if}
    {#if d.kind !== 'pc' && d.status !== 'unauthorized'}<Menu label={t('dev.more', { name: d.name })} items={more} />{/if}
  </div>
</div>

<style>
  .dev { display: flex; gap: 16px; align-items: center; padding: 16px 18px; border-top: 1px solid var(--line); flex-wrap: wrap; }
  .dev:first-of-type { border-top: 0; }
  .dev.att { background: color-mix(in oklab, var(--warn-soft), transparent 30%); }
  .di { width: 48px; height: 48px; border-radius: 12px; display: grid; place-items: center; background: var(--surface-2); color: var(--text-2); flex: none; }
  .dm { flex: 1 1 280px; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .nm { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .nm b { font-size: 16px; }
  .path { overflow-wrap: anywhere; }
  .facts { display: flex; gap: 6px 14px; flex-wrap: wrap; font-size: 13.5px; color: var(--text-2); }
  .facts span { display: inline-flex; gap: 5px; align-items: center; }
  .facts .warn { color: var(--warn); font-weight: 700; }
  .da { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; align-items: center; }
</style>
