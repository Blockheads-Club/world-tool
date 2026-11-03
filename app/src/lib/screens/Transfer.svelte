<script lang="ts">
  import { bytes, clock, day, pct, when } from '../format';
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { Choice, Device, Stage } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Check from '../ui/Check.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Icon from '../ui/Icon.svelte';
  import Notice from '../ui/Notice.svelte';
  import PhoneWait from '../ui/PhoneWait.svelte';
  import Progress from '../ui/Progress.svelte';
  import Seg from '../ui/Seg.svelte';
  import Steps from '../ui/Steps.svelte';

  const f = store.transfer;
  let picker = $state(false);
  let iosTried = $state(false);

  const libCount = $derived(store.worlds?.length ?? null);
  $effect(() => {
    if (store.worlds === null) void store.loadWorlds();
  });

  const sources = $derived(store.devices.filter((d) => d.status === 'ready'));
  const targets = $derived(store.devices.filter((d) => d.kind !== 'ios' && d.status !== 'offline'));
  const iosTargets = $derived(store.devices.filter((d) => d.kind === 'ios'));
  const target = $derived(f.target);
  const needsPhone = $derived(f.plan ? f.plan.needs_phone : target?.kind === 'android' && !(target.root && store.settings?.root_mode !== 'never'));
  const backupOn = $derived(store.settings?.backup_before_write ?? true);

  const steps = $derived([t('tr.step.choose'), t('tr.step.review'), t('tr.step.backup'), ...(needsPhone ? [t('tr.step.phone')] : []), t('tr.step.write')]);
  const stage = $derived(f.progress?.stage);
  const current = $derived.by(() => {
    if (f.stage === 'choose') return 0;
    if (f.stage === 'review') return 1;
    if (f.stage === 'before') return 2;
    if (f.stage === 'done') return steps.length;
    if (stage === 'phone_backup' || stage === 'backing_up' || stage === undefined) return 2;
    if (stage === 'preparing' || stage === 'phone_restore') return needsPhone ? 3 : 2;
    return steps.length - 1;
  });

  const icon = (d: Device): IconName => (d.kind === 'pc' ? 'laptop' : d.kind === 'ios' ? 'archive' : 'phone');
  const via = (d: Device) => (d.connection === 'wifi' ? 'Wi-Fi' : 'USB');
  function fromHint(d: Device): string {
    if (d.kind === 'pc') return t('tr.pcHint', { n: d.world_count ?? 0 });
    if (d.kind === 'ios') return t('tr.iosHint', { date: day(d.backup_date ?? 0) });
    return t('tr.phoneHint', { via: via(d) });
  }
  function toHint(d: Device): string {
    if (d.status === 'unauthorized') return t('dev.unauth');
    if (d.kind === 'pc') return t('tr.pcTo', { v: d.game_version ?? '-' });
    return t('tr.phoneTo', { os: d.os_version ?? '-', v: d.game_version ?? '-', via: via(d) });
  }

  const S = $derived(f.summary);
  const runSteps: { stage: Stage; label: () => string }[] = [
    { stage: 'backing_up', label: () => t('tr.run.backup') },
    { stage: 'preparing', label: () => t('tr.run.prepare') },
    { stage: 'writing', label: () => (target?.kind === 'pc' ? t('tr.run.writePc') : t('tr.run.write')) },
    { stage: 'checking', label: () => t('tr.run.check') }
  ];
  const order: Stage[] = ['phone_backup', 'backing_up', 'preparing', 'phone_restore', 'writing', 'checking'];
  const at = $derived(order.indexOf(stage ?? 'phone_backup'));
  const shownRun = $derived(runSteps.filter((s) => s.stage !== 'backing_up' || f.seen.includes('backing_up')));
</script>

{#snippet radio(name: string, checked: boolean, onpick: () => void, ic: IconName, title: string, hint: string, disabled = false)}
  <label class={['radio', checked && 'on', disabled && 'dis']}>
    <input type="radio" {name} {checked} {disabled} onchange={(e) => { e.currentTarget.checked = checked; onpick(); }} />
    <span class="rd" aria-hidden="true"></span>
    <Icon name={ic} />
    <span class="rt"><b>{title}</b><span class="hint">{hint}</span></span>
  </label>
{/snippet}

<Header title={t('tr.title')} sub={t('tr.sub')} />
<div class="main">
  <Steps {steps} {current} />

  {#if f.stage === 'choose'}
    <div class="grid2">
      <section class="card pad" aria-labelledby="tr-from">
        <h2 id="tr-from">{t('tr.from')}</h2>
        <div class="list" role="radiogroup" aria-labelledby="tr-from">
          {@render radio('from', f.from === 'library', () => void f.setFrom('library'), 'library', t('nav.worlds'), libCount === null ? t('common.loading') : t('tr.libHint', { n: libCount }))}
          {@render radio('from', f.from === 'files', () => void f.setFrom('files'), 'file', t('tr.files'), f.from === 'files' ? t('tr.filesChosen', { n: f.files.length }) : t('tr.filesHint'))}
          {#each sources as d (d.id)}
            {@render radio('from', f.from === d.id, () => void f.setFrom(d.id), icon(d), d.kind === 'ios' ? t('tr.iosName', { name: d.name }) : d.name, fromHint(d))}
          {/each}
        </div>
      </section>
      <section class="card pad" aria-labelledby="tr-to">
        <h2 id="tr-to">{t('tr.to')}</h2>
        <div class="list" role="radiogroup" aria-labelledby="tr-to">
          {#each targets as d (d.id)}
            {@render radio('to', f.to === d.id, () => (f.to = d.id), icon(d), d.name, d.id === f.from ? t('tr.isSource') : toHint(d), !f.canWrite(d))}
          {/each}
          {#each iosTargets as d (d.id)}
            <div class="radio dis" role="radio" aria-checked="false" aria-disabled="true" tabindex="0" onclick={() => (iosTried = true)} onkeydown={(e) => e.key === ' ' && (iosTried = true)}>
              <span class="rd" aria-hidden="true"></span><Icon name="archive" />
              <span class="rt"><b>{t('tr.iosName', { name: d.name })}</b><span class="hint">{t('tr.iosNo')}</span></span>
              <Badge tone="info" icon="lock">{t('dev.readOnly')}</Badge>
            </div>
          {/each}
        </div>
        {#if iosTried}<Notice icon="lock" title={t('tr.iosNoTitle')}>{t('tr.iosNoText')}</Notice>{/if}
      </section>
    </div>
    <div class="card wbar">
      {#if f.available === 'unread'}
        <Notice title={t('tr.unreadTitle')}>
          {t('tr.unreadText')}
          {#snippet actions()}<Button size="sm" variant="primary" icon="download" onclick={() => store.read.start(store.devices.find((d) => d.id === f.from) as Device)}>{t('dev.read')}</Button>{/snippet}
        </Notice>
      {:else}
        <b>{t('tr.worlds')}</b>
        {#if f.available === null}<span class="spin"></span>{:else if f.picked.length === 0}<span class="hint">{t('tr.noneChosen')}</span>{/if}
        {#each f.pickedWorlds as w (w.id)}<Badge tone="primary" icon="check">{w.name}</Badge>{/each}
        <button class="link" disabled={f.available === null} onclick={() => (picker = true)}>{t('tr.change')}</button>
      {/if}
      <span class="sp"></span>
      <Button variant="primary" icon="chevr" disabled={!f.to || f.picked.length === 0 || !Array.isArray(f.available)} onclick={() => void f.review()}>{t('tr.nextReview')}</Button>
    </div>
  {:else if f.stage === 'review' && f.plan}
    <div class="grid2 wide">
      <div class="card">
        <h2 class="ph">{t('tr.after', { name: f.plan.target, n: S.total })}</h2>
        <div class="scroll">
          <table class="tbl">
            <thead><tr><th>{t('read.world')}</th><th>{t('tr.whereFrom')}</th><th>{t('read.size')}</th><th>{t('tr.result')}</th></tr></thead>
            <tbody>
              {#each f.plan.items as i (i.key)}
                <tr class={[i.result === 'conflict' && 'rsel']}>
                  <td><b>{i.name}</b>{#if i.result === 'conflict' && f.choices[i.key] === 'keep_both'}<br /><span class="hint">{t('tr.copyNamed', { name: i.copy_name ?? '' })}</span>{/if}</td>
                  <td>
                    {#if i.result === 'stays'}{t('tr.already')}
                    {:else if i.result === 'added'}{t('tr.new')}
                    {:else}{t('tr.both')} <Badge tone="warn" icon="alert">{i.conflict === 'same_id' ? t('tr.sameId') : t('tr.sameName')}</Badge>{/if}
                  </td>
                  <td class="mono">{bytes(i.size)}</td>
                  <td>
                    {#if i.result === 'stays'}<Badge icon="check">{t('tr.stays')}</Badge>
                    {:else if i.result === 'added'}<Badge tone="ok" icon="plus">{t('tr.added')}</Badge>
                    {:else}
                      <Seg size="sm" label={t('tr.whatToDo', { name: i.name })} value={f.choices[i.key]} onchange={(v: Choice) => (f.choices[i.key] = v)}
                        options={[{ value: 'keep_both', label: t('choice.keep_both') }, { value: 'replace', label: t('choice.replace') }, { value: 'skip', label: t('choice.skip') }]} />
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
      <div class="card pad">
        <b>{t('tr.summary')}</b>
        <dl class="kv">
          <dt>{t('tr.added')}</dt><dd>{S.added}</dd>
          <dt>{t('tr.stay')}</dt><dd>{S.stay}</dd>
          {#if S.conflicts}<dt>{t('tr.sameName')}</dt><dd>{t('tr.conflictsN', { n: S.conflicts, replaced: S.replaced, skipped: S.skipped })}</dd>{/if}
          <dt>{t('read.size')}</dt><dd>{bytes(f.plan.size)}</dd>
        </dl>
        <p class="hint">{t('tr.reviewHint')}</p>
        <div class="kpi">
          <Button variant="primary" icon="chevr" onclick={() => (f.stage = 'before')}>{t('common.next')}</Button>
          <Button variant="ghost" onclick={() => (f.stage = 'choose')}>{t('common.back')}</Button>
        </div>
      </div>
    </div>
  {:else if f.stage === 'before' && f.plan}
    <div class="grid2">
      <div class="card pad">
        <h2>{t('tr.beforeTitle')}</h2>
        <Notice tone="bad" role="alert" title={target?.kind === 'pc' ? t('tr.replacesPc') : t('tr.replaces')}>{t('tr.replacesText', { n: S.total })}</Notice>
        <ul class="ol">
          {#if target?.kind === 'android'}
            <li>{t('tr.installed')} <b>{target.game_version ?? '-'} ✓</b></li>
            <li>{t('tr.closeGame')}</li>
            <li>{t('tr.keepCable')}</li>
          {:else}
            <li>{t('tr.pcClosed')}</li>
          {/if}
        </ul>
        {#if target?.kind === 'android'}<Check bind:checked={f.gameClosed} label={t('common.gameClosed')} />{/if}
        <div class="kpi">
          <Button variant="primary" size="lg" icon="upload" disabled={target?.kind === 'android' && !f.gameClosed} onclick={() => void f.write()}>{backupOn ? t('tr.backupWrite') : t('tr.writeNow')}</Button>
          <Button variant="ghost" size="lg" onclick={() => (f.stage = 'review')}>{t('common.back')}</Button>
        </div>
      </div>
      {#if backupOn}
        <div class="card pad">
          <div class="res"><span class="ri"><Icon name="shield" size={28} /></span><span><h2>{t('tr.autoBackup')}</h2><span class="muted">{t('tr.autoBackupText')}</span></span></div>
          <dl class="kv">
            <dt>{t('tr.savedTo')}</dt><dd>{t('nav.backups')}</dd>
            <dt>{t('tr.name')}</dt><dd>{f.plan.backup_name} · {when(Date.now(), Date.now(), true)}</dd>
            <dt>{t('read.size')}</dt><dd>{t('tr.about', { size: bytes(f.plan.backup_size) })}</dd>
          </dl>
          <p class="hint"><b>{t('tr.undo')}</b> {needsPhone ? t('tr.undoTextPhone') : t('tr.undoText')}</p>
        </div>
      {:else}
        <Notice tone="warn" title={t('tr.noBackupTitle')}>{t('tr.noBackupText')}</Notice>
      {/if}
    </div>
  {:else if f.stage === 'running'}
    {#if stage === 'phone_backup' || stage === 'phone_restore'}
      <PhoneWait screen={stage === 'phone_backup' ? 'backup' : 'restore'} elapsed={f.progress?.elapsed ?? 0}
        title={stage === 'phone_backup' ? t('tr.phoneBackupTitle') : t('tr.phoneRestoreTitle')}
        text={stage === 'phone_backup' ? t('tr.phoneBackupText') : t('tr.phoneRestoreText')}>
        {#snippet before()}{#if stage === 'phone_restore' && f.seen.includes('backing_up')}<p class="muted ok"><Icon name="check" size={18} />{t('tr.backupSaved')}</p>{/if}{/snippet}
        {#snippet actions()}<Button icon="x" onclick={() => void f.cancel()}>{t('common.cancel')}</Button>{/snippet}
      </PhoneWait>
    {:else}
      <div class="grid2">
        <div class="card pad">
          <Progress title={t('tr.writingTo', { n: S.total, name: f.plan?.target ?? '' })} value={stage === 'writing' || stage === 'backing_up' ? pct(f.progress?.done, f.progress?.total) : null}
            detail={stage === 'writing' && f.progress?.total ? t('tr.writingDetail', { done: bytes(f.progress.done ?? 0), total: bytes(f.progress.total) }) : ''}>
            {#snippet actions()}<Button size="sm" onclick={() => void f.cancel()}>{t('common.cancel')}</Button>{/snippet}
          </Progress>
        </div>
        <div class="card pad">
          <ol class="run">
            {#each shownRun as s (s.stage)}
              {@const i = order.indexOf(s.stage)}
              <li class={[i < at && 'done', i === at && 'cur']}>
                {#if i < at}<Icon name="check" />{:else if i === at}<span class="spin"></span>{:else}<Icon name="clock" />{/if}
                {#if i === at}<b>{s.label()}</b>{:else}{s.label()}{/if}
              </li>
            {/each}
          </ol>
        </div>
      </div>
    {/if}
  {:else if f.stage === 'done' && f.result}
    {@const r = f.result}
    <div class="card pad" role="status">
      <div class="res">
        <span class="ri"><Icon name="check" size={28} /></span>
        <div>
          <h2>{t('tr.doneTitle', { name: r.target, n: r.total })}</h2>
          <p class="muted">
            {#if r.added.length}{t('tr.doneAdded', { names: r.added.join(', ') })}{/if}
            {#each r.kept_both as k (k)} {t('tr.doneKept', { name: k })}{/each}
            {#if r.replaced.length} {t('tr.doneReplaced', { names: r.replaced.join(', ') })}{/if}
            {target?.kind === 'pc' ? t('tr.checkPc') : t('tr.checkPhone')}
          </p>
        </div>
      </div>
      {#if r.backup_id}
        <Notice icon="history" title={t('tr.wrong')}>
          {t('tr.wrongText', { time: clock(r.backup_time ?? 0) })}
          {#snippet actions()}<Button size="sm" icon="history" onclick={() => { store.restoreId = r.backup_id; store.go('backups'); }}>{t('tr.restoreBackup')}</Button>{/snippet}
        </Notice>
      {/if}
      <div class="kpi">
        <Button variant="primary" onclick={() => store.go('devices')}>{t('common.done')}</Button>
        <Button variant="ghost" icon="history" onclick={() => store.go('activity')}>{t('tr.showActivity')}</Button>
      </div>
    </div>
  {/if}
</div>

{#if picker && Array.isArray(f.available)}
  <Dialog title={t('tr.pickTitle')} icon="library" onclose={() => (picker = false)}>
    {#if f.available.length === 0}<p class="muted">{t('tr.pickNone')}</p>{/if}
    <ul class="pick">
      {#each f.available as w (w.id)}
        <li><Check label={w.name} checked={f.picked.includes(w.id)} onchange={(v) => f.togglePick(w.id, v)} /><span class="hint">{bytes(w.size)}</span></li>
      {/each}
    </ul>
    {#snippet footer()}<Button variant="primary" onclick={() => (picker = false)}>{t('common.done')}</Button>{/snippet}
  </Dialog>
{/if}

<style>
  .list { display: flex; flex-direction: column; gap: 10px; }
  .radio { display: flex; gap: 12px; align-items: center; padding: 12px 16px; border: 2px solid var(--line); border-radius: 14px; background: var(--surface); cursor: pointer; position: relative; }
  .radio.on { border-color: var(--primary); background: var(--primary-soft); }
  .radio.dis { opacity: 0.6; cursor: not-allowed; }
  .radio input { position: absolute; opacity: 0; }
  .radio:has(input:focus-visible), .radio:focus-visible { outline: 3px solid var(--focus); outline-offset: 2px; }
  .rd { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--line-strong); flex: none; background: var(--surface); }
  .radio.on .rd { border: 7px solid var(--primary); }
  .rt { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .wbar { padding: 14px 18px; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .wbar :global(.notice) { flex: 1 1 100%; }
  .sp { flex: 1; }
  .wide { grid-template-columns: minmax(0, 2fr) minmax(0, 1fr); }
  .ph { padding: 16px 18px 6px; font-size: 18px; }
  .scroll { overflow-x: auto; }
  .run { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 12px; }
  .run li { display: flex; gap: 10px; align-items: center; color: var(--text-3); }
  .run li.done { color: var(--ok); }
  .run li.cur { color: var(--text); }
  .ok { display: flex; gap: 6px; align-items: center; color: var(--ok); }
  .pick { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .pick li { display: flex; justify-content: space-between; gap: 10px; align-items: center; }
  @media (max-width: 1100px) { .wide { grid-template-columns: minmax(0, 1fr); } }
</style>
