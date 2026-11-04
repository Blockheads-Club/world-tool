import { beforeEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({
  invoke: vi.fn(async (..._a: unknown[]): Promise<unknown> => []),
  listen: vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => {
    h.listeners.push(cb);
    return () => {};
  }),
  listeners: [] as ((e: { payload: unknown }) => void)[],
  drop: null as null | ((e: { payload: unknown }) => void),
  win: {
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    isMaximized: vi.fn(async () => true),
    onResized: vi.fn(async () => () => {}),
    onFocusChanged: vi.fn(async (cb: (e: { payload: boolean }) => void) => {
      cb({ payload: false });
      return () => {};
    }),
    onDragDropEvent: vi.fn(async (cb: (e: { payload: unknown }) => void) => {
      h.drop = cb;
      return () => {};
    })
  }
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: h.invoke,
  Channel: class {
    onmessage: (p: unknown) => void = () => {};
  }
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: h.listen }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => h.win }));

import { COMMANDS, inTauri, tauriBackend as b, win } from './api';

describe('tauri backend', () => {
  beforeEach(() => {
    h.invoke.mockClear();
    h.listeners = [];
  });

  it('calls one command per method with camelCase arguments', async () => {
    const cb = vi.fn();
    const calls: [() => Promise<unknown>, string, Record<string, unknown> | undefined][] = [
      [() => b.appInfo(), 'app_info', undefined],
      [() => b.getSettings(), 'get_settings', undefined],
      [() => b.setSettings({ theme: 'dark' }), 'set_settings', { patch: { theme: 'dark' } }],
      [() => b.listDevices(), 'list_devices', undefined],
      [() => b.wifiDiscover(), 'wifi_discover', undefined],
      [() => b.wifiPair('a:1', '123456'), 'wifi_pair', { address: 'a:1', code: '123456' }],
      [() => b.pairedDevices(), 'paired_devices', undefined],
      [() => b.forgetPaired('x'), 'forget_paired', { id: 'x' }],
      [() => b.restartAdb(), 'restart_adb', undefined],
      [() => b.adbStatus(), 'adb_status', undefined],
      [() => b.adbDownloadInfo(), 'adb_download_info', undefined],
      [() => b.openDeviceManager(), 'open_device_manager', undefined],
      [() => b.iosBackups(null), 'ios_backups', { folder: null }],
      [() => b.iosOpen('i', 'pw'), 'ios_open', { id: 'i', password: 'pw' }],
      [() => b.diagnostics(), 'diagnostics', undefined],
      [() => b.saveToLibrary('r', ['w']), 'save_to_library', { readId: 'r', worldIds: ['w'] }],
      [() => b.freeSpace('/f'), 'free_space', { folder: '/f' }],
      [() => b.cancel('op'), 'cancel', { op: 'op' }],
      [() => b.sourceWorlds({ kind: 'library' }), 'source_worlds', { source: { kind: 'library' } }],
      [() => b.transferPlan({ kind: 'library' }, ['w'], 'd'), 'transfer_plan', { source: { kind: 'library' }, worldIds: ['w'], targetId: 'd' }],
      [() => b.listWorlds(), 'list_worlds', undefined],
      [() => b.renameWorld('w', 'n'), 'rename_world', { id: 'w', name: 'n' }],
      [() => b.duplicateWorld('w'), 'duplicate_world', { id: 'w' }],
      [() => b.deleteWorld('w'), 'delete_world', { id: 'w' }],
      [() => b.showInFolder('/p'), 'show_in_folder', { path: '/p' }],
      [() => b.inspectImport(['/p']), 'inspect_import', { paths: ['/p'] }],
      [() => b.importWorlds({ paths: [], choices: {} }), 'import_worlds', { req: { paths: [], choices: {} } }],
      [() => b.listBackups(), 'list_backups', undefined],
      [() => b.deleteBackup('b'), 'delete_backup', { id: 'b' }],
      [() => b.listActivity(), 'list_activity', undefined],
      [() => b.clearActivity(), 'clear_activity', undefined],
      [() => b.gameFiles(), 'game_files', undefined],
      [() => b.clearGameFiles(), 'clear_game_files', undefined],
      [() => b.pickFolder(null), 'pick_folder', { start: null }],
      [() => b.pickImport(true), 'pick_import', { folders: true }],
      [() => b.pickApk(), 'pick_apk', undefined],
      [() => b.checkUpdate(), 'check_update', undefined],
      [() => b.restartApp(), 'restart_app', undefined]
    ];
    for (const [call, name, args] of calls) {
      await call();
      expect(h.invoke).toHaveBeenLastCalledWith(name, ...(args ? [args] : []));
      expect(COMMANDS).toContain(name);
    }
  });

  it('streams progress over a channel', async () => {
    const cb = vi.fn();
    const ops: [() => Promise<unknown>, string][] = [
      [() => b.read('o', { device_id: 'd', password: '' }, cb), 'read_worlds'],
      [() => b.exportWorlds('o', { read_id: null, world_ids: [], format: 'bhworld', folder: '/' }, cb), 'export_worlds'],
      [() => b.transfer('o', { plan_id: 'p', choices: {} }, cb), 'transfer'],
      [() => b.restoreBackup('o', 'b', 'd', cb), 'restore_backup'],
      [() => b.useApk('o', '/a.apk', cb), 'use_apk'],
      [() => b.apkFromDevice('o', 'd', cb), 'apk_from_device'],
      [() => b.reextract('o', cb), 'reextract'],
      [() => b.downloadUpdate('o', cb), 'download_update'],
      [() => b.downloadAdb('o', cb), 'download_adb']
    ];
    for (const [call, name] of ops) {
      await call();
      const [cmd, args] = h.invoke.mock.lastCall as [string, { onProgress: { onmessage: (p: unknown) => void } }];
      expect(cmd).toBe(name);
      args.onProgress.onmessage({ stage: 'reading' });
      expect(cb).toHaveBeenLastCalledWith({ stage: 'reading' });
      cb.mockClear();
    }
  });

  it('forwards events: devices, opened files (also those from start-up) and drag and drop', async () => {
    const dev = vi.fn();
    await b.onDevicesChanged(dev);
    h.listeners[0]({ payload: [{ id: 'x' }] });
    expect(dev).toHaveBeenCalledWith([{ id: 'x' }]);

    const open = vi.fn();
    h.invoke.mockResolvedValueOnce(['/a.bhworld']);
    await b.onOpenFiles(open);
    expect(open).toHaveBeenCalledWith(['/a.bhworld']);
    h.listeners[1]({ payload: ['/b.bhworld'] });
    expect(open).toHaveBeenLastCalledWith(['/b.bhworld']);
    h.invoke.mockResolvedValueOnce([]);
    const quiet = vi.fn();
    await b.onOpenFiles(quiet);
    expect(quiet).not.toHaveBeenCalled();

    const drop = vi.fn();
    await b.onDragDrop(drop);
    h.drop?.({ payload: { type: 'enter', paths: ['a', 'b'] } });
    h.drop?.({ payload: { type: 'over', position: {} } });
    h.drop?.({ payload: { type: 'drop', paths: ['a'] } });
    h.drop?.({ payload: { type: 'leave' } });
    expect(drop.mock.calls.map((c) => c[0])).toEqual([{ type: 'over', count: 2 }, { type: 'drop', paths: ['a'] }, { type: 'leave' }]);
  });

  it('drives the window and knows where it runs', async () => {
    await win.minimize();
    await win.toggleMaximize();
    await win.close();
    expect(await win.isMaximized()).toBe(true);
    await win.onResized(() => {});
    const f = vi.fn();
    await win.onFocus(f);
    expect(f).toHaveBeenCalledWith(false);
    expect(h.win.minimize).toHaveBeenCalled();
    expect(inTauri()).toBe(false);
    const { chooseBackend, MockBackend } = await import('./mock-api');
    expect(chooseBackend('')).toBeInstanceOf(MockBackend);
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    expect(inTauri()).toBe(true);
    expect(chooseBackend('')).toBe(b);
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });
});
