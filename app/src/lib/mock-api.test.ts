import { describe, expect, it, vi } from 'vitest';
import { MockBackend, mockFromUrl, mockPreview } from './mock-api';
import type { Progress } from './types';

const m = (o: ConstructorParameters<typeof MockBackend>[0] = {}) => new MockBackend({ tick: 0, ...o });
const stages = (list: Progress[]) => [...new Set(list.map((p) => p.stage))];

describe('mock backend', () => {
  it('serves an editor session', async () => {
    const b = m();
    const w = await b.editorOpen((await b.listWorlds())[0].id);
    expect((await b.editorState(w.session)).unsaved).toBe(0);
    expect((await b.editorBlockheads(w.session)).length).toBe(2);
    await b.editorClose(w.session);
    await expect(b.editorState(w.session)).rejects.toMatchObject({ code: 'not_found' });
  });

  it('starts with the design data, or empty / fresh', async () => {
    const full = m();
    expect((await full.listDevices()).length).toBe(7);
    expect((await full.listWorlds()).length).toBe(7);
    expect((await full.listBackups()).length).toBe(3);
    expect((await full.listActivity()).length).toBe(6);
    expect((await full.gameFiles()).state).toBe('ready');
    expect((await full.getSettings()).first_run_done).toBe(true);
    expect(await full.appInfo()).toEqual({ version: '0.1.0', os: 'linux' });
    const empty = m({ scenario: 'empty' });
    expect(await empty.listDevices()).toEqual([]);
    expect(await empty.pairedDevices()).toEqual([]);
    expect((await empty.gameFiles()).state).toBe('not_set');
    const fresh = m({ scenario: 'fresh' });
    expect((await fresh.listDevices()).length).toBe(7);
    expect(await fresh.listWorlds()).toEqual([]);
    expect(new MockBackend().tick).toBe(120);
    expect(mockPreview('a', 'cave')).toMatch(/^data:image\/svg\+xml,/);
    expect(mockPreview('a', 'snow')).not.toBe(mockPreview('a', 'desert'));
  });

  it('reads a phone with the phone confirmation, saves and exports', async () => {
    const b = m();
    const ev: Progress[] = [];
    const r = await b.read('op', { device_id: 'pixel8', password: '' }, (p) => ev.push(p));
    expect(stages(ev)).toEqual(['phone_backup', 'reading']);
    expect(r.worlds.length).toBe(5);
    const saved = await b.saveToLibrary(r.read_id, ['w-sunny']);
    expect(saved[0]).toMatchObject({ name: 'Sunny Isle', origin: 'phone', source_detail: 'USB' });
    const ex: Progress[] = [];
    const out = await b.exportWorlds('op', { read_id: r.read_id, world_ids: ['w-deep', 'w-castle'], format: 'bhworld', folder: '/x' }, (p) => ex.push(p));
    expect(out.files.map((f) => f.name)).toEqual(['Deep Mine.bhworld', 'Castle Build.bhworld']);
    expect(stages(ex)).toEqual(['exporting', 'preview']);
    const lib = await b.listWorlds();
    const srv = await b.exportWorlds('op', { read_id: null, world_ids: [lib[0].id], format: 'server', folder: '/x' }, () => {});
    expect(srv.files[0].name).toBe(lib[0].name);
    expect(srv.saved).toEqual([]);
    await expect(b.saveToLibrary('nope', [])).rejects.toMatchObject({ code: 'not_found' });
    expect(await b.freeSpace('/x')).toBeGreaterThan(0);
  });

  it('reads root phones, Wi-Fi tablets, Blockheads PC and iPhone backups without the phone step', async () => {
    const b = m();
    for (const id of ['redmi', 'pc', 'iphone', 'tab']) {
      const ev: Progress[] = [];
      const r = await b.read('op', { device_id: id, password: '' }, (p) => ev.push(p));
      expect(ev.some((p) => p.stage === 'phone_backup')).toBe(id === 'tab');
      const s = await b.saveToLibrary(r.read_id, r.worlds.map((w) => w.id));
      expect(s[0].origin).toBe(id === 'pc' ? 'pc' : id === 'iphone' ? 'ios' : 'phone');
    }
    await b.setSettings({ root_mode: 'never' });
    const ev: Progress[] = [];
    await b.read('op', { device_id: 'redmi', password: '' }, (p) => ev.push(p));
    expect(ev[0].stage).toBe('phone_backup');
    b.phoneWorlds = {};
    const none = await b.read('op', { device_id: 'pixel8', password: '' }, () => {});
    expect(none.worlds).toEqual([]);
  });

  it('rejects reads with the designed errors and can be cancelled', async () => {
    const b = m();
    await expect(b.read('o', { device_id: 'moto', password: '' }, () => {})).rejects.toMatchObject({ code: 'not_allowed' });
    await expect(b.read('o', { device_id: 'pixel6a', password: '' }, () => {})).rejects.toMatchObject({ code: 'disconnected' });
    await expect(b.read('o', { device_id: 'nope', password: '' }, () => {})).rejects.toMatchObject({ code: 'not_found' });
    b.plug({ ...(await b.listDevices())[0], id: 'nogame', game_version: null });
    await expect(b.read('o', { device_id: 'nogame', password: '' }, () => {})).rejects.toMatchObject({ code: 'game_not_installed' });
    b.fail.read = { code: 'declined' };
    await expect(b.read('o', { device_id: 'pixel8', password: '' }, () => {})).rejects.toMatchObject({ code: 'declined' });
    expect(b.fail.read).toBeUndefined();
    const p = b.read('c', { device_id: 'pixel8', password: '' }, () => {});
    await b.cancel('c');
    await expect(p).rejects.toMatchObject({ code: 'cancelled' });
    b.fail.export = { code: 'disk_full' };
    await expect(b.exportWorlds('o', { read_id: null, world_ids: [], format: 'bhworld', folder: '/' }, () => {})).rejects.toMatchObject({ code: 'disk_full' });
  });

  it('pairs over Wi-Fi, lists and forgets paired phones, tells listeners', async () => {
    const b = m();
    const seen = vi.fn();
    const off = await b.onDevicesChanged(seen);
    await expect(b.wifiPair('bad', '1')).rejects.toMatchObject({ code: 'pair_failed' });
    const d = await b.wifiPair('10.0.0.9:4000', '123 456');
    expect(d.name).toBe('Android (10.0.0.9)');
    expect(seen).toHaveBeenCalled();
    const again = await b.wifiPair('192.168.1.42:37115', '123456');
    expect(again.id).toBe('tab');
    expect((await b.pairedDevices()).length).toBe(3);
    await b.forgetPaired(d.id);
    expect((await b.pairedDevices()).length).toBe(2);
    b.fail.pair = { code: 'pair_failed' };
    await expect(b.wifiPair('10.0.0.9:4000', '123456')).rejects.toMatchObject({ code: 'pair_failed' });
    off();
    expect(await b.wifiDiscover()).toContainEqual({ name: 'Galaxy Tab S6 Lite', address: '192.168.1.42:37115' });
    await b.restartAdb();
    await b.openDeviceManager();
    expect(await b.diagnostics()).toContain('Ada’s Pixel 8');
    b.plug({ ...d, game_version: null });
    expect(await b.diagnostics()).toContain('game -');
  });

  it('opens iPhone backups', async () => {
    const b = m();
    expect((await b.iosBackups(null)).length).toBe(2);
    await expect(b.iosOpen('iphone', '')).rejects.toMatchObject({ code: 'wrong_password' });
    expect((await b.iosOpen('iphone', 'pw')).id).toBe('iphone');
    const ipad = await b.iosOpen('ipad', '');
    expect(ipad).toMatchObject({ kind: 'ios', read_only: true });
    await expect(b.iosOpen('nope', '')).rejects.toMatchObject({ code: 'not_found' });
    b.fail.ios = { code: 'io' };
    await expect(b.iosOpen('ipad', '')).rejects.toMatchObject({ code: 'io' });
  });

  it('plans and runs transfers with conflicts, backups and phone steps', async () => {
    const b = m();
    const lib = await b.sourceWorlds({ kind: 'library' });
    const sunny = lib.find((w) => w.name === 'Sunny Isle')!;
    const pirate = lib.find((w) => w.name === 'Pirate Cove')!;
    const plan = await b.transferPlan({ kind: 'library' }, [sunny.id, pirate.id], 'pixel8');
    expect(plan.items[0]).toMatchObject({ result: 'conflict', conflict: 'same_name', copy_name: 'Sunny Isle (2)' });
    expect(plan.items.at(-1)).toMatchObject({ name: 'Pirate Cove', result: 'added' });
    expect(plan.needs_phone).toBe(true);
    const ev: Progress[] = [];
    const res = await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, (p) => ev.push(p));
    expect(stages(ev)).toEqual(['phone_backup', 'backing_up', 'preparing', 'phone_restore', 'writing', 'checking']);
    expect(res).toMatchObject({ total: 7, added: ['Pirate Cove'], kept_both: ['Sunny Isle (2)'] });
    expect(res.backup_id).toBeTruthy();

    const p2 = await b.transferPlan({ kind: 'library' }, [sunny.id], 'pixel8');
    const r2 = await b.transfer('o', { plan_id: p2.plan_id, choices: { [sunny.id]: 'replace' } }, () => {});
    expect(r2.replaced).toEqual(['Sunny Isle']);
    const p3 = await b.transferPlan({ kind: 'library' }, [sunny.id], 'pixel8');
    const r3 = await b.transfer('o', { plan_id: p3.plan_id, choices: { [sunny.id]: 'skip' } }, () => {});
    expect(r3.added.length + r3.replaced.length).toBe(0);

    // Blockheads PC: no phone, world count updated; backups off: no backup.
    await b.setSettings({ backup_before_write: false });
    const p4 = await b.transferPlan({ kind: 'library' }, [pirate.id], 'pc');
    expect(p4.needs_phone).toBe(false);
    const ev4: Progress[] = [];
    const r4 = await b.transfer('o', { plan_id: p4.plan_id, choices: {} }, (p) => ev4.push(p));
    expect(r4.backup_id).toBeNull();
    expect(stages(ev4)).toEqual(['preparing', 'writing', 'checking']);
    expect((await b.listDevices()).find((d) => d.id === 'pc')!.world_count).toBe(6);

    // A target without worlds yet, root target
    const p5 = await b.transferPlan({ kind: 'library' }, [pirate.id], 'redmi');
    expect(p5.needs_phone).toBe(false);
    b.phoneWorlds.redmi = undefined as never;
    await b.transfer('o', { plan_id: p5.plan_id, choices: {} }, () => {});
    expect(b.phoneWorlds.redmi.length).toBe(1);
  });

  it('writes to a phone that has no worlds yet', async () => {
    const b = m();
    const d = await b.wifiPair('10.0.0.7:5555', '123456');
    const lib = await b.listWorlds();
    const plan = await b.transferPlan({ kind: 'library' }, [lib[0].id], d.id);
    const r = await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, () => {});
    expect(r.total).toBe(1);
    expect((await b.listBackups())[0]).toMatchObject({ device_id: d.id, worlds: 0 });
  });

  it('keeps only the newest backups per device', async () => {
    const b = m();
    await b.setSettings({ backups_to_keep: 1 });
    const lib = await b.listWorlds();
    const plan = await b.transferPlan({ kind: 'library' }, [lib[3].id], 'pixel8');
    await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, () => {});
    expect((await b.listBackups()).filter((x) => x.device_id === 'pixel8').length).toBe(1);
  });

  it('fails transfers as asked, with the backup to undo', async () => {
    const b = m();
    await expect(b.transfer('o', { plan_id: 'nope', choices: {} }, () => {})).rejects.toMatchObject({ code: 'not_found' });
    await expect(b.transferPlan({ kind: 'library' }, [], 'iphone')).rejects.toMatchObject({ code: 'io' });
    const lib = await b.listWorlds();
    const plan = await b.transferPlan({ kind: 'library' }, [lib[3].id], 'pixel8');
    b.fail.transfer = { code: 'disconnected', percent: 61 };
    const e = await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, () => {}).catch((x) => x);
    expect(e).toMatchObject({ code: 'disconnected', device: 'Ada’s Pixel 8' });
    expect(e.backup_id).toBeTruthy();
    await b.setSettings({ backup_before_write: false });
    b.fail.transfer = { code: 'io', detail: 'x' };
    const e2 = await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, () => {}).catch((x) => x);
    expect(e2.backup_id).toBeUndefined();
    b.fail.transfer = { code: 'io' };
    await b.transfer('o', { plan_id: plan.plan_id, choices: {} }, () => {}).catch(() => {});
    expect((await b.listActivity())[0]).toMatchObject({ result: 'failed', detail: 'io' });
  });

  it('offers source worlds of files, phones (after a read), Blockheads PC', async () => {
    const b = m();
    const files = await b.sourceWorlds({ kind: 'files', paths: ['/a/X.bhworld', '/a/notes.txt'] });
    expect(files.map((w) => w.name)).toEqual(['X']);
    await expect(b.sourceWorlds({ kind: 'device', device_id: 'tab' })).rejects.toMatchObject({ code: 'not_found' });
    await b.read('o', { device_id: 'tab', password: '' }, () => {});
    expect((await b.sourceWorlds({ kind: 'device', device_id: 'tab' })).length).toBe(2);
    expect((await b.sourceWorlds({ kind: 'device', device_id: 'pc' })).length).toBe(5);
    b.phoneWorlds = {};
    expect(await b.sourceWorlds({ kind: 'device', device_id: 'pc' })).toEqual([]);
    b.fail.source = { code: 'io' };
    await expect(b.sourceWorlds({ kind: 'library' })).rejects.toMatchObject({ code: 'io' });
    // a world id clash (same world, other name) → same_id
    const w = (await b.sourceWorlds({ kind: 'library' }))[0];
    b.phoneWorlds.pc = [{ ...w, name: 'Renamed', id: (w as unknown as { world_id: string }).world_id }];
    const plan = await b.transferPlan({ kind: 'library' }, [w.id], 'pc');
    expect(plan.items[0].conflict).toBe('same_id');
  });

  it('manages Worlds and imports with conflicts', async () => {
    const b = m();
    const [w] = await b.listWorlds();
    expect((await b.renameWorld(w.id, '  New  ')).name).toBe('New');
    expect((await b.renameWorld(w.id, '   ')).name).toBe('New');
    const c = await b.duplicateWorld(w.id);
    expect(c.name).toBe('New (2)');
    await b.deleteWorld(c.id);
    await expect(b.deleteWorld('nope')).rejects.toMatchObject({ code: 'not_found' });
    await b.showInFolder('/x');
    const items = await b.inspectImport(['/d/Frozen Peaks.bhworld', '/d/Sunny Isle.bhworld', '/s/Deep Mine', '/d/notes.txt']);
    expect(items.map((i) => i.status)).toEqual(['new', 'new', 'conflict', 'invalid']);
    expect(items[2]).toMatchObject({ kind: 'server', status: 'conflict', conflict: 'same_name' });
    expect(items[3].kind).toBe('invalid');
    const sunnyFile = (await b.inspectImport(['/d/Castle Build.bhworld']))[0];
    expect(sunnyFile).toMatchObject({ status: 'conflict', conflict: 'same_id', existing: 'Castle Build' });
    const before = (await b.listWorlds()).length;
    const out = await b.importWorlds({ paths: ['/d/Frozen Peaks.bhworld', '/d/Castle Build.bhworld', '/s/Deep Mine', '/d/notes.txt'], choices: { '/s/Deep Mine': 'replace' } });
    expect(out.map((x) => x.name)).toEqual(['Frozen Peaks', 'Castle Build (2)', 'Deep Mine']);
    expect(out[2].source_detail).toBe('server');
    expect((await b.listWorlds()).length).toBe(before + 2);
    const skip = await b.importWorlds({ paths: ['/d/Frozen Peaks.bhworld'], choices: { '/d/Frozen Peaks.bhworld': 'skip' } });
    expect(skip).toEqual([]);
    b.fail.import = { code: 'io' };
    await expect(b.inspectImport([])).rejects.toMatchObject({ code: 'io' });
  });

  it('restores and deletes backups, clears activity', async () => {
    const b = m();
    const ev: Progress[] = [];
    await b.restoreBackup('o', 'b1', 'pixel8', (p) => ev.push(p));
    expect(stages(ev)).toEqual(['phone_backup', 'phone_restore', 'writing']);
    await expect(b.restoreBackup('o', 'nope', 'pixel8', () => {})).rejects.toMatchObject({ code: 'not_found' });
    b.fail.restore = { code: 'declined' };
    await expect(b.restoreBackup('o', 'b1', 'pixel8', () => {})).rejects.toMatchObject({ code: 'declined' });
    await b.deleteBackup('b1');
    expect((await b.listBackups()).some((x) => x.id === 'b1')).toBe(false);
    await b.clearActivity();
    expect(await b.listActivity()).toEqual([]);
  });

  it('checks and extracts the APK, pulls it from a phone, re-extracts and clears', async () => {
    const b = m({ scenario: 'fresh' });
    const ev: Progress[] = [];
    const g = await b.useApk('o', '/d/TheBlockheads.apk', (p) => ev.push(p));
    expect(g).toMatchObject({ state: 'ready', version: '1.7.6' });
    expect(stages(ev)).toEqual(['apk_checking', 'apk_extracting']);
    await expect(b.useApk('o', '/d/Other.apk', () => {})).rejects.toMatchObject({ code: 'apk_wrong_app', name: 'Other.apk' });
    b.fail.apk = { code: 'apk_damaged' };
    await expect(b.useApk('o', '/d/TheBlockheads.apk', () => {})).rejects.toMatchObject({ code: 'apk_damaged' });
    const ev2: Progress[] = [];
    await b.apkFromDevice('o', 'pixel8', (p) => ev2.push(p));
    expect(ev2[0].stage).toBe('apk_pulling');
    await b.reextract('o', () => {});
    expect((await b.clearGameFiles()).state).toBe('not_set');
    b.game.apk_path = null;
    await expect(b.reextract('o', () => {})).rejects.toMatchObject({ code: 'apk_missing' });
  });

  it('pickers return examples or what a test set', async () => {
    const b = m();
    expect(await b.pickFolder(null)).toBe('/home/ada/Documents/Blockheads worlds');
    expect(await b.pickFolder('/s')).toBe('/s');
    expect(await b.pickImport(false)).toHaveLength(2);
    expect(await b.pickImport(true)).toEqual(['/home/ada/server/skyblock_world']);
    expect(await b.pickApk()).toMatch(/TheBlockheads\.apk$/);
    b.picks = { folder: null, files: [], apk: null };
    expect(await b.pickFolder(null)).toBeNull();
    expect(await b.pickImport(true)).toEqual([]);
    expect(await b.pickApk()).toBeNull();
  });

  it('offers updates, downloads, fails as asked', async () => {
    const b = m();
    expect(await b.checkUpdate()).toBeNull();
    b.update = { version: '0.3.0', notes: ['a'] };
    expect(await b.checkUpdate()).toMatchObject({ version: '0.3.0' });
    const ev: Progress[] = [];
    await b.downloadUpdate('o', (p) => ev.push(p));
    expect(ev.at(-1)).toMatchObject({ done: ev.at(-1)!.total });
    b.fail.update = { code: 'offline' };
    await expect(b.downloadUpdate('o', () => {})).rejects.toMatchObject({ code: 'offline' });
    b.fail.check = { code: 'offline' };
    await expect(b.checkUpdate()).rejects.toMatchObject({ code: 'offline' });
    await b.restartApp();
  });

  it('hands opened and dragged files to listeners', async () => {
    const b = m();
    const o = vi.fn();
    const d = vi.fn();
    const off1 = await b.onOpenFiles(o);
    const off2 = await b.onDragDrop(d);
    b.openFiles(['/a']);
    b.drag({ type: 'leave' });
    expect(o).toHaveBeenCalledWith(['/a']);
    expect(d).toHaveBeenCalledWith({ type: 'leave' });
    off1();
    off2();
    b.openFiles(['/b']);
    expect(o).toHaveBeenCalledTimes(1);
  });

  it('reads dev options from the URL', async () => {
    const a = mockFromUrl('?scenario=empty&tick=5&fail=read:declined&update=0.3.0');
    expect(a.tick).toBe(5);
    expect(a.fail.read).toMatchObject({ code: 'declined' });
    expect(a.update?.version).toBe('0.3.0');
    expect(await a.listDevices()).toEqual([]);
    const b = mockFromUrl('?scenario=fresh');
    expect(b.tick).toBe(120);
    expect(mockFromUrl('').settings.first_run_done).toBe(true);
    expect(mockFromUrl('?scenario=odd').settings.first_run_done).toBe(true);
  });
});
