<script lang="ts">
  import { onMount } from 'svelte';
  import { dash, day, zero } from '../format';
  import { t, type Params } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import type { Key } from '../messages';
  import Header from '../layout/Header.svelte';
  import { store, type Guide } from '../store.svelte';
  import type { IosBackup, RootMode, WifiCandidate } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import Phone from '../ui/Phone.svelte';
  import Seg from '../ui/Seg.svelte';
  import { toast } from '../ui/toast.svelte';
  import { copyText } from '../util';

  const GUIDES: { id: Guide; icon: IconName; sub?: boolean }[] = [
    { id: 'usb', icon: 'usb', sub: true },
    { id: 'wifi', icon: 'wifi', sub: true },
    { id: 'root', icon: 'root', sub: true },
    { id: 'pc', icon: 'laptop', sub: true },
    { id: 'ios', icon: 'archive', sub: true }
  ];
  const EXTRA: { id: Guide; icon: IconName }[] = [
    { id: 'driver', icon: 'monitor' },
    { id: 'trouble', icon: 'info' }
  ];

  // USB: a device that shows up while this guide is open.
  let known = $state<string[]>([]);
  onMount(() => {
    known = store.readyPhones.map((d) => d.id);
  });
  const arrived = $derived(store.readyPhones.find((d) => !known.includes(d.id)) ?? null);

  // Wi-Fi pairing
  let address = $state('');
  let code = $state('');
  let pairing = $state(false);
  let pairError = $state(false);
  let found = $state<WifiCandidate[]>([]);
  $effect(() => {
    if (store.guide === 'wifi') void store.api.wifiDiscover().then((c) => (found = c));
  });
  async function pair() {
    pairing = true;
    pairError = false;
    try {
      const d = await store.api.wifiPair(address, code);
      toast(t('wifi.paired', { name: d.name }));
      code = '';
    } catch {
      pairError = true;
    } finally {
      pairing = false;
    }
  }

  // iPhone backups
  let backups = $state<IosBackup[]>([]);
  let chosen = $state<string | null>(null);
  let password = $state('');
  let opening = $state(false);
  let wrong = $state(false);
  $effect(() => {
    if (store.guide === 'ios') void loadIos(null);
  });
  async function loadIos(folder: string | null) {
    backups = await store.api.iosBackups(folder);
    chosen = backups[0]?.id ?? null;
  }
  async function browseIos() {
    const f = await store.api.pickFolder(null);
    if (f) await loadIos(f);
  }
  const chosenBackup = $derived(backups.find((b) => b.id === chosen) ?? null);
  async function openIos() {
    opening = true;
    wrong = false;
    try {
      const d = await store.api.iosOpen(chosen as string, password);
      password = '';
      store.read.start(d);
    } catch {
      wrong = true;
    } finally {
      opening = false;
    }
  }

  // Blockheads PC
  const pc = $derived(store.devices.find((d) => d.kind === 'pc') ?? null);
  async function pcFolder() {
    const f = await store.api.pickFolder(pc?.path ?? null);
    if (f) {
      await store.saveSettings({ pc_folder: f });
      await store.refreshDevices();
    }
  }

  async function restartAdb() {
    await store.api.restartAdb();
    toast(t('trouble.restarted'));
    await store.refreshDevices();
  }
  async function diag() {
    toast((await copyText(await store.api.diagnostics())) ? t('common.copied') : t('common.copyFailed'));
  }

  const os = $derived(store.info.os);
</script>

{#snippet steps(list: { title: Key; text: Key; params?: Params }[], doneUpTo: number)}
  <ol class="gsteps">
    {#each list as s, i (s.title)}
      <li>
        <span class={['n', i < doneUpTo && 'done', i === doneUpTo && 'cur']}>{#if i < doneUpTo}<Icon name="check" size={16} />{:else}{i + 1}{/if}</span>
        <div class="st"><b>{t(s.title)}</b><div class="muted">{t(s.text, s.params)}</div></div>
      </li>
    {/each}
  </ol>
{/snippet}

<Header title={t('con.title')} sub={t('con.sub')}>
  {#snippet actions()}<Button variant="ghost" icon="back" onclick={() => store.go('devices')}>{t('con.back')}</Button>{/snippet}
</Header>
<div class="main">
  <div class="layout">
    <nav class="card gnav" aria-label={t('con.types')}>
      {#each GUIDES as g (g.id)}
        <button class="gv tall" aria-current={store.guide === g.id ? 'true' : undefined} onclick={() => (store.guide = g.id)}>
          <Icon name={g.icon} />
          <span class="lbl"><span>{t(`guide.${g.id}` as Key)}</span><span class="hint">{t(`guide.${g.id}Sub` as Key)}</span></span>
        </button>
      {/each}
      <div class="nvsep" role="separator"></div>
      {#each EXTRA as g (g.id)}
        <button class="gv" aria-current={store.guide === g.id ? 'true' : undefined} onclick={() => (store.guide = g.id)}>
          <Icon name={g.icon} /><span class="lbl">{t(`guide.${g.id}` as Key)}</span>
        </button>
      {/each}
    </nav>

    <section class="card pad guide" aria-labelledby="gt">
      <h2 id="gt">{t(`guide.${store.guide}` as Key)}</h2>
      {#if store.guide === 'usb'}
        <div class="row">
          <div class="grow">
            {@render steps([
              { title: 'usb.s1', text: 'usb.s1t' },
              { title: 'usb.s2', text: 'usb.s2t' },
              { title: 'usb.s3', text: 'usb.s3t' },
              { title: 'usb.s4', text: 'usb.s4t' }
            ], 3)}
          </div>
          <Phone screen="allow" />
        </div>
        {#if arrived}
          <Notice tone="ok" icon="check" role="status" title={t('usb.found', { name: arrived.name })}>
            {t('dev.game', { v: dash(arrived.game_version) })}
            {#snippet actions()}<Button size="sm" variant="primary" icon="download" onclick={() => store.read.start(arrived)}>{t('dev.read')}</Button>{/snippet}
          </Notice>
        {:else}
          <div class="notice-wait" role="status"><span class="spin"></span><div><b>{t('usb.waiting')}</b><p class="muted">{t('usb.waitingText')}</p></div></div>
        {/if}
        {#if os === 'windows'}
          <p class="hint">{t('usb.noWindows')} <button class="link" onclick={() => (store.guide = 'driver')}>{t('usb.driver')}</button> · <button class="link" onclick={() => (store.guide = 'trouble')}>{t('guide.trouble')}</button></p>
        {:else}
          <p class="hint"><button class="link" onclick={() => (store.guide = 'trouble')}>{t('dev.trouble')}</button></p>
        {/if}
      {:else if store.guide === 'wifi'}
        {@render steps([
          { title: 'wifi.s1', text: 'wifi.s1t' },
          { title: 'wifi.s2', text: 'wifi.s2t' },
          { title: 'wifi.s3', text: 'wifi.s3t' }
        ], 2)}
        <form class="card pairf" onsubmit={(e) => { e.preventDefault(); void pair(); }}>
          <div class="ffield"><label for="ip">{t('wifi.ip')}</label><input id="ip" class="input mono" bind:value={address} placeholder="192.168.1.42:37115" autocomplete="off" /></div>
          <div class="ffield"><label for="pc">{t('wifi.code')}</label><input id="pc" class="input mono" bind:value={code} inputmode="numeric" placeholder="482 915" autocomplete="off" aria-invalid={pairError || undefined} /></div>
          <div class="pa">
            <Button type="submit" variant="primary" icon="link" loading={pairing} disabled={!address.trim() || !code.trim()}>{t('wifi.pair')}</Button>
            <span class="hint">{t('wifi.after')}</span>
          </div>
          {#if pairError}<p class="bad" role="alert">{t('err.pair_failed')}</p>{/if}
        </form>
        {#each found as c (c.address)}
          <Notice title={t('wifi.foundTitle')}>
            {t('wifi.foundText', { name: c.name, ip: c.address.split(':')[0] })}
            {#snippet actions()}<Button size="sm" onclick={() => (address = c.address)}>{t('wifi.use')}</Button>{/snippet}
          </Notice>
        {/each}
      {:else if store.guide === 'root'}
        <p>{t('root.intro')}</p>
        {@render steps([
          { title: 'root.s1', text: 'root.s1t' },
          { title: 'root.s2', text: 'root.s2t' }
        ], 1)}
        <div class="card sub">
          <b id="ur">{t('set.root')}</b>
          <Seg label={t('set.root')} value={store.settings!.root_mode} onchange={(v: RootMode) => void store.saveSettings({ root_mode: v })}
            options={[{ value: 'auto', label: t('root.auto') }, { value: 'never', label: t('root.never') }]} />
          <p class="hint">{t('root.autoHint')}</p>
        </div>
        <Notice tone="warn" title={t('root.warnTitle')}>{t('root.warnText')}</Notice>
      {:else if store.guide === 'pc'}
        {#if pc}
          <Notice tone="ok" icon="check" role="status" title={t('pc.found')}>
            {t('pc.foundText', { n: zero(pc.world_count), v: dash(pc.game_version) })}
            {#snippet actions()}
              <Button size="sm" variant="primary" icon="download" onclick={() => store.read.start(pc)}>{t('dev.read')}</Button>
              <Button size="sm" onclick={() => store.go('devices')}>{t('con.back')}</Button>
            {/snippet}
          </Notice>
        {:else}
          <Notice tone="warn" title={t('pc.notFound')}>{t('pc.notFoundText')}</Notice>
        {/if}
        <div class="ffield">
          <label for="pp">{t('pc.folder')}</label>
          <div class="pathbox"><input id="pp" class="input" value={store.settings?.pc_folder ?? pc?.path ?? ''} readonly /><Button icon="folder" onclick={() => void pcFolder()}>{t('common.change')}</Button></div>
          <span class="hint">{t('pc.folderHint')}</span>
        </div>
        <Notice tone="warn" title={t('pc.closeTitle')}>{t('pc.closeText')}</Notice>
      {:else if store.guide === 'ios'}
        <Notice icon="eye" title={t('ios.roTitle')}>{t('ios.roText')}</Notice>
        <div class="card">
          <div class="ih"><b>{t('ios.inFolder')}</b><Button size="sm" icon="folder" onclick={() => void browseIos()}>{t('common.browse')}</Button></div>
          {#if backups.length}
            <table class="tbl">
              <thead><tr><th><span class="sr">{t('common.choose')}</span></th><th>{t('ios.device')}</th><th>{t('ios.date')}</th><th>{t('ios.protection')}</th></tr></thead>
              <tbody>
                {#each backups as b (b.id)}
                  <tr class={[chosen === b.id && 'rsel']}>
                    <td><input type="radio" name="iosb" aria-label={b.name} checked={chosen === b.id} onchange={() => (chosen = b.id)} /></td>
                    <td><b>{b.name}</b><br /><span class="hint">{b.model}</span></td>
                    <td>{day(b.date)}</td>
                    <td>{#if b.encrypted}<Badge tone="warn" icon="lock">{t('dev.encrypted')}</Badge>{:else}<Badge>{t('ios.notEncrypted')}</Badge>{/if}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <p class="muted pad1">{t('ios.none')}</p>
          {/if}
        </div>
        {#if chosenBackup}
          <form class="card pairf one" onsubmit={(e) => { e.preventDefault(); void openIos(); }}>
            {#if chosenBackup.encrypted}
              <div class="ffield">
                <label for="bp">{t('ios.password', { name: chosenBackup.name })}</label>
                <input id="bp" class="input" type="password" bind:value={password} aria-invalid={wrong || undefined} aria-describedby="bph" />
                <span class="hint" id="bph">{wrong ? t('err.wrong_password') : t('ios.passwordHint')}</span>
              </div>
            {/if}
            <div><Button type="submit" variant="primary" icon="lock" loading={opening}>{t('ios.open')}</Button></div>
          </form>
        {/if}
        <p class="hint">{t('ios.folders')}</p>
      {:else if store.guide === 'driver'}
        <p>{t('drv.intro')}</p>
        {@render steps([
          { title: 'drv.s1', text: 'drv.s1t' },
          { title: 'drv.s2', text: 'drv.s2t' },
          { title: 'drv.s3', text: 'drv.s3t' }
        ], 1)}
        <div class="kpi"><Button icon="extl" onclick={() => void store.api.openDeviceManager()}>{t('drv.open')}</Button></div>
      {:else}
        {#each ['1', '2', '3', '4', '5'] as n (n)}
          <details class="card det" open={n === '3'}>
            <summary>{t(`trouble.q${n}` as Key)}</summary>
            <p class="muted">{t(`trouble.a${n}` as Key)}</p>
          </details>
        {/each}
        <div class="kpi">
          <Button icon="refresh" onclick={() => void restartAdb()}>{t('trouble.restart')}</Button>
          <Button variant="ghost" icon="copy" onclick={() => void diag()}>{t('common.copyDiag')}</Button>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .layout { display: grid; grid-template-columns: 280px minmax(0, 1fr); gap: 20px; align-items: start; }
  .gnav { padding: 8px; display: flex; flex-direction: column; gap: 4px; }
  .gv { display: flex; align-items: center; gap: 12px; min-height: 44px; padding: 6px 12px; border-radius: var(--r-md); border: 0; background: transparent; color: var(--text-2); font: 700 15px/1.25 var(--f-body); text-align: left; cursor: pointer; }
  .gv.tall { min-height: 60px; }
  .gv:hover { background: var(--surface-2); color: var(--text); }
  .gv[aria-current='true'] { background: var(--primary-soft); color: var(--primary-ink); }
  .lbl { display: flex; flex-direction: column; min-width: 0; }
  .lbl .hint { font-weight: 400; }
  .nvsep { height: 2px; background: var(--line); margin: 4px 6px; }
  .guide h2 { font-size: 22px; }
  .row { display: flex; gap: 24px; align-items: flex-start; flex-wrap: wrap; }
  .grow { flex: 1 1 360px; min-width: 0; }
  .gsteps { list-style: none; margin: 0; padding: 0; }
  .gsteps li { display: flex; gap: 14px; align-items: flex-start; padding: 12px 0; border-top: 1px solid var(--line); }
  .gsteps li:first-child { border-top: 0; }
  .n { width: 30px; height: 30px; border-radius: 50%; display: grid; place-items: center; font: 800 14px var(--f-body); background: var(--surface-3); color: var(--text-2); flex: none; }
  .n.done { background: var(--ok); color: #fff; }
  .n.cur { background: var(--primary); color: var(--on-primary); }
  .st { display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0; }
  .st .muted { font-size: 14.5px; }
  .notice-wait { display: flex; gap: 14px; align-items: flex-start; padding: 14px 16px; border-radius: 14px; background: var(--info-soft); border: 2px solid color-mix(in oklab, var(--info), transparent 65%); }
  .notice-wait .muted { font-size: 14.5px; }
  .pairf { padding: 18px; display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1fr); gap: 14px; align-items: end; }
  .pairf.one { grid-template-columns: minmax(0, 1fr); }
  .pa { grid-column: 1 / -1; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .bad { grid-column: 1 / -1; color: var(--bad); font-weight: 700; }
  .sub { padding: 16px 18px; display: flex; flex-direction: column; gap: 10px; }
  .ih { padding: 14px 18px 4px; display: flex; justify-content: space-between; gap: 10px; align-items: center; flex-wrap: wrap; }
  .pad1 { padding: 8px 18px 16px; }
  .det { padding: 14px 18px; }
  .det summary { font-weight: 700; cursor: pointer; }
  .det p { margin-top: 8px; }
  @media (max-width: 1100px) { .layout { grid-template-columns: minmax(0, 1fr); } .gnav { flex-direction: row; flex-wrap: wrap; } .gv .hint { display: none; } .nvsep { display: none; } }
</style>
