<script lang="ts">
  import { bytes, pct, when } from '../format';
  import { t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import type { ExportFormat, Origin, Progress as P, World } from '../types';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';
  import Empty from '../ui/Empty.svelte';
  import Icon from '../ui/Icon.svelte';
  import Menu from '../ui/Menu.svelte';
  import Notice from '../ui/Notice.svelte';
  import Progress from '../ui/Progress.svelte';
  import Thumb from '../ui/Thumb.svelte';
  import { toast } from '../ui/toast.svelte';
  import { opId, shortId } from '../util';
  import { toAppError } from '../errors';

  type Filter = 'all' | Origin;
  type Sort = 'new' | 'old' | 'name' | 'size';
  let query = $state('');
  let filter = $state<Filter>('all');
  let sort = $state<Sort>('new');
  let selId = $state<string | null>(null);
  let renaming = $state<World | null>(null);
  let newName = $state('');
  let deleting = $state<World | null>(null);
  let exporting = $state<{ name: string; progress: P | null } | null>(null);

  $effect(() => {
    if (store.worlds === null) void store.loadWorlds();
  });

  const all = $derived(store.worlds!);
  const count = (o: Origin) => all.filter((w) => w.origin === o).length;
  const FILTERS: { id: Filter; label: () => string }[] = [
    { id: 'all', label: () => t('worlds.all') },
    { id: 'phone', label: () => t('worlds.phones') },
    { id: 'pc', label: () => 'Blockheads PC' },
    { id: 'ios', label: () => t('worlds.ios') },
    { id: 'file', label: () => t('worlds.files') }
  ];
  const shown = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase();
    const list = all.filter((w) => (filter === 'all' || w.origin === filter) && (!q || w.name.toLocaleLowerCase().includes(q)));
    const by: Record<Sort, (a: World, b: World) => number> = {
      new: (a, b) => b.added - a.added,
      old: (a, b) => a.added - b.added,
      name: (a, b) => a.name.localeCompare(b.name),
      size: (a, b) => b.size - a.size
    };
    return [...list].sort(by[sort]);
  });
  const sel = $derived(all.find((w) => w.id === selId) ?? null);
  const total = $derived(all.reduce((s, w) => s + w.size, 0));
  const originIcon = { phone: 'phone', pc: 'laptop', ios: 'archive', file: 'file' } as const;

  function source(w: World): string {
    return w.origin === 'file' ? t('worlds.fromFile', { name: w.source }) : w.source;
  }

  async function rename() {
    const w = renaming as World;
    renaming = null;
    await store.api.renameWorld(w.id, newName);
    await store.loadWorlds();
  }

  async function duplicate(w: World) {
    const c = await store.api.duplicateWorld(w.id);
    toast(t('worlds.duplicated', { name: c.name }));
    await store.loadWorlds();
  }

  async function remove() {
    const w = deleting as World;
    deleting = null;
    await store.api.deleteWorld(w.id);
    if (selId === w.id) selId = null;
    toast(t('worlds.deleted', { name: w.name }));
    await store.loadWorlds();
  }

  async function exportOne(w: World, format: ExportFormat) {
    const folder = await store.api.pickFolder(null);
    if (!folder) return;
    exporting = { name: w.name, progress: null };
    try {
      const r = await store.api.exportWorlds(opId('export'), { read_id: null, world_ids: [w.id], format, folder }, (p) => {
        if (exporting) exporting.progress = p;
      });
      toast(t('worlds.exported', { name: r.files[0].name }), { label: t('common.showInFolder'), run: () => void store.api.showInFolder(r.folder) });
    } catch (e) {
      const damaged = toAppError(e).code === 'world_damaged';
      store.fail(e, () => void (damaged ? store.api.deleteWorld(w.id).then(() => store.loadWorlds()) : exportOne(w, format)));
    } finally {
      exporting = null;
    }
  }

  const copyTo = (w: World) => store.transfer.start({ from: 'library', worlds: [w.id] });
  const menu = (w: World) => [
    { label: t('worlds.details'), icon: 'info' as const, run: () => (selId = w.id) },
    { label: t('worlds.rename'), icon: 'pencil' as const, run: () => { renaming = w; newName = w.name; } },
    { label: t('worlds.duplicate'), icon: 'copy' as const, run: () => void duplicate(w) },
    { label: t('worlds.asBhworld'), icon: 'file' as const, run: () => void exportOne(w, 'bhworld') },
    { label: t('worlds.asServer'), icon: 'server' as const, run: () => void exportOne(w, 'server') },
    { label: t('common.delete'), icon: 'trash' as const, danger: true, run: () => (deleting = w) }
  ];
</script>

<Header title={t('nav.worlds')} sub={store.worlds ? t('worlds.sub', { n: all.length, size: bytes(total) }) : t('worlds.subEmpty')}>
  {#snippet actions()}
    <Button icon="download" onclick={() => void store.importer.open()}>{t('worlds.import')}</Button>
    <Button variant="ghost" icon="folder" onclick={() => void store.api.showInFolder(store.settings!.worlds_folder)}>{t('worlds.openFolder')}</Button>
  {/snippet}
</Header>
<div class="main">
  {#if store.worlds === null}
    <p class="hint" role="status">{t('worlds.loading')}</p>
    <div class="lgrid" aria-hidden="true">
      {#each [1, 2, 3, 4, 5, 6] as i (i)}<div class="wcard"><span class="skel sk1"></span><span class="skel sk2"></span><span class="skel sk3"></span></div>{/each}
    </div>
  {:else if all.length === 0}
    <Empty title={t('worlds.emptyTitle')} text={t('worlds.emptyText')}>
      <Button variant="primary" icon="download" onclick={() => store.go('devices')}>{t('worlds.readDevice')}</Button>
      <Button icon="download" onclick={() => void store.importer.open()}>{t('worlds.import')}</Button>
    </Empty>
  {:else}
    {#if store.savedEdit}
      <Notice tone="ok" icon="check" role="status" title={t('worlds.savedTitle', { name: store.savedEdit.world })}>
        {t('worlds.savedText', { n: store.savedEdit.changes })}
        {#snippet actions()}
          <Button size="sm" variant="primary" icon="upload" onclick={() => { const w = all.find((x) => x.name === store.savedEdit?.world); store.savedEdit = null; store.transfer.start({ from: 'library', worlds: w ? [w.id] : [] }); }}>{t('worlds.copyTo')}</Button>
          <Button size="sm" variant="ghost" icon="pencil" onclick={() => { const w = all.find((x) => x.name === store.savedEdit?.world); store.savedEdit = null; if (w) store.edit(w); }}>{t('worlds.editAgain')}</Button>
        {/snippet}
      </Notice>
    {/if}
    <div class="tools">
      <div class="srch"><Icon name="search" /><input class="input" type="search" placeholder={t('worlds.search')} aria-label={t('worlds.search')} bind:value={query} /></div>
      <div class="chips" role="group" aria-label={t('worlds.filter')}>
        {#each FILTERS as fl (fl.id)}
          <button class="chip" aria-pressed={filter === fl.id} onclick={() => (filter = fl.id)}>{fl.label()} <span class="n">{fl.id === 'all' ? all.length : count(fl.id)}</span></button>
        {/each}
      </div>
      <select class="sel" aria-label={t('worlds.sort')} bind:value={sort}>
        <option value="new">{t('worlds.sortNew')}</option>
        <option value="old">{t('worlds.sortOld')}</option>
        <option value="name">{t('worlds.sortName')}</option>
        <option value="size">{t('worlds.sortSize')}</option>
      </select>
    </div>
    <div class={[sel && 'split']}>
      {#if shown.length === 0}
        <p class="muted">{t('worlds.noMatch')}</p>
      {/if}
      <div class="lgrid" role="list">
        {#each shown as w (w.id)}
          <article class={['wcard', selId === w.id && 'on']} role="listitem" aria-label={w.name}>
            <button class="pv" aria-label={t('worlds.showDetails', { name: w.name })} aria-pressed={selId === w.id} onclick={() => (selId = selId === w.id ? null : w.id)}><Thumb src={w.preview} name={w.name} large /></button>
            <span class="mn"><Menu label={t('worlds.actionsFor', { name: w.name })} items={menu(w)} /></span>
            <b>{w.name}</b>
            <span class="meta"><span><Icon name={originIcon[w.origin]} size={16} />{source(w)}</span></span>
            <span class="meta"><span>{when(w.added)}</span><span class="mono">{bytes(w.size)}</span>{#if w.edited}<Badge tone="ok">{t('worlds.editedNow')}</Badge>{/if}</span>
            <div class="kpi">
              <Button size="sm" icon="pencil" onclick={() => store.edit(w)}>{t('common.edit')}</Button>
              <Button size="sm" icon="upload" label={t('worlds.copyAria', { name: w.name })} onclick={() => copyTo(w)} />
            </div>
          </article>
        {/each}
      </div>
      {#if sel}
        <aside class="card panel" aria-label={t('worlds.detailsOf', { name: sel.name })}>
          <div class="ph"><Thumb src={sel.preview} name={sel.name} large /><Button variant="ghost" size="sm" icon="x" label={t('common.close')} onclick={() => (selId = null)} /></div>
          <div><h2>{sel.name}</h2><span class="hint">{t('read.gameV', { v: sel.game_version })} · {sel.mode === 'expert' ? t('mode.expert') : t('mode.normal')}</span></div>
          <dl class="kv">
            <dt>{t('worlds.from')}</dt><dd>{`${sel.source}${sel.source_detail && sel.source_detail !== 'server' ? ` · ${sel.source_detail}` : ''}`}</dd>
            <dt>{t('worlds.read')}</dt><dd>{when(sel.added, Date.now(), true)}</dd>
            <dt>{t('read.lastPlayed')}</dt><dd>{sel.last_played ? when(sel.last_played, Date.now(), true) : t('read.never')}</dd>
            <dt>{t('read.created')}</dt><dd>{when(sel.created)}</dd>
            <dt>{t('read.size')}</dt><dd>{bytes(sel.size)}</dd>
            <dt>{t('worlds.worldId')}</dt><dd class="mono" title={sel.world_id}>{shortId(sel.world_id)}</dd>
          </dl>
          <div class="acol">
            <Button variant="primary" icon="pencil" onclick={() => store.edit(sel)}>{t('common.edit')}</Button>
            <Button icon="upload" onclick={() => copyTo(sel)}>{t('worlds.copyTo')}</Button>
            <Button icon="file" onclick={() => void exportOne(sel, 'bhworld')}>{t('worlds.asBhworld')}</Button>
            <Button icon="server" onclick={() => void exportOne(sel, 'server')}>{t('worlds.asServer')}</Button>
          </div>
          <div class="kpi">
            <Button size="sm" variant="ghost" icon="pencil" onclick={() => { renaming = sel; newName = sel!.name; }}>{t('worlds.rename')}</Button>
            <Button size="sm" variant="ghost" icon="copy" onclick={() => void duplicate(sel)}>{t('worlds.duplicate')}</Button>
            <Button size="sm" variant="ghost" icon="folder" onclick={() => void store.api.showInFolder(store.settings!.worlds_folder)}>{t('common.showInFolder')}</Button>
            <Button size="sm" variant="quiet-bad" icon="trash" onclick={() => (deleting = sel)}>{t('common.delete')}</Button>
          </div>
        </aside>
      {/if}
    </div>
  {/if}
</div>

{#if renaming}
  <Dialog title={t('worlds.renameTitle')} icon="pencil" onclose={() => (renaming = null)}>
    <form id="rn" onsubmit={(e) => { e.preventDefault(); void rename(); }} class="ffield">
      <label for="rn-i">{t('worlds.newName')}</label>
      <input id="rn-i" class="input" bind:value={newName} data-autofocus />
    </form>
    {#snippet footer()}
      <Button onclick={() => (renaming = null)}>{t('common.cancel')}</Button>
      <Button variant="primary" type="submit" disabled={!newName.trim()} onclick={() => void rename()}>{t('worlds.rename')}</Button>
    {/snippet}
  </Dialog>
{/if}
{#if deleting}
  <Dialog title={t('worlds.deleteTitle', { name: deleting.name })} icon="trash" tone="bad" alert onclose={() => (deleting = null)}>
    <p>{t('worlds.deleteText')}</p>
    {#snippet footer()}
      <Button onclick={() => (deleting = null)} autofocus>{t('common.cancel')}</Button>
      <Button variant="danger" icon="trash" onclick={() => void remove()}>{t('common.delete')}</Button>
    {/snippet}
  </Dialog>
{/if}
{#if exporting}
  <Dialog title={t('worlds.exportingTitle', { name: exporting.name })} icon="upload" busy>
    <Progress title={t('worlds.exportingTitle', { name: exporting.name })} value={pct(exporting.progress?.done, exporting.progress?.total)} />
  </Dialog>
{/if}

<style>
  .tools { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .lgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(230px, 1fr)); gap: 14px; }
  .wcard { display: flex; flex-direction: column; gap: 8px; padding: 10px 10px 12px; border: 2px solid var(--line); border-radius: 16px; background: var(--surface); position: relative; }
  .wcard.on { border-color: var(--primary); box-shadow: 0 0 0 3px color-mix(in oklab, var(--primary), transparent 70%); }
  .wcard b { font-size: 16px; overflow-wrap: anywhere; }
  .pv { padding: 0; border: 0; background: none; cursor: pointer; border-radius: 10px; display: block; }
  .mn { position: absolute; right: 14px; top: 14px; border-radius: 10px; background: color-mix(in oklab, var(--surface), transparent 10%); }
  .meta { display: flex; gap: 6px 10px; flex-wrap: wrap; font-size: 13px; color: var(--text-2); align-items: center; }
  .meta span { display: inline-flex; gap: 5px; align-items: center; }
  .split { display: grid; grid-template-columns: minmax(0, 1fr) 340px; gap: 18px; align-items: start; }
  .panel { position: sticky; top: 0; padding: 18px; display: flex; flex-direction: column; gap: 12px; }
  .ph { display: flex; gap: 6px; align-items: flex-start; }
  .acol { display: flex; flex-direction: column; gap: 8px; }
  .acol :global(.btn) { justify-content: flex-start; }
  .sk1 { aspect-ratio: 21 / 8; }
  .sk2 { height: 18px; width: 60%; }
  .sk3 { height: 14px; width: 80%; }
  @media (max-width: 1100px) { .split { grid-template-columns: minmax(0, 1fr) 300px; } }
</style>
