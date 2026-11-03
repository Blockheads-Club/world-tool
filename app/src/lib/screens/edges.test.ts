import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as layout from '../../routes/+layout';
import { store } from '../store.svelte';
import { setup } from '../test-utils';
import type { Device } from '../types';
import Dialog from '../ui/Dialog.svelte';
import Progress from '../ui/Progress.svelte';
import Steps from '../ui/Steps.svelte';
import { toast, toasts } from '../ui/toast.svelte';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('edge cases', () => {
  it('page config, toasts expire, busy dialogs stay, progress without a number', async () => {
    expect(layout).toMatchObject({ ssr: false, prerender: false });
    vi.useFakeTimers();
    toasts.items = [];
    toast('hello');
    expect(toasts.items).toHaveLength(1);
    vi.advanceTimersByTime(6000);
    expect(toasts.items).toHaveLength(0);
    vi.useRealTimers();
    const onclose = vi.fn();
    render(Dialog, { title: 'Busy', busy: true, onclose });
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Close' })).toBeNull();
    cleanup();
    const pr = render(Progress, { title: 'Busy bar', value: 40 });
    await pr.rerender({ title: 'Busy bar', value: null });
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
    await pr.rerender({ title: 'Busy bar', value: 60 });
    cleanup();
    const st = render(Steps, { steps: ['A', 'B'], current: 0 });
    await st.rerender({ steps: ['C', 'D'], current: 1 });
    expect(screen.getByText('D')).toBeInTheDocument();
  });

  it('devices: failed refresh, nulls, Wi-Fi guide from a row, no-game phone, offline without date', async () => {
    const { mock } = await setup({
      before: (m) => {
        const base = m.devices[0];
        m.devices = [
          { ...base, id: 'w', name: 'Wifi phone', connection: 'wifi', address: null, os_version: null },
          { ...base, id: 'ng', name: 'No game', game_version: null },
          { ...base, id: 'off', name: 'Old', status: 'offline', last_seen: null, connection: 'usb' },
          { ...m.devices[5], path: null, world_count: null, game_version: null },
          { ...m.devices[6], os_version: null, backup_date: null }
        ];
      }
    });
    expect(screen.getAllByText('Game not installed').length).toBe(2);
    expect(screen.getByText('Google Pixel 8 · Android -')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Wifi phone' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Connection guide' }));
    expect(store.guide).toBe('wifi');
    store.go('devices');
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(await screen.findByRole('button', { name: 'More actions for Wifi phone' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Read worlds' })[2]);
    expect(store.read.device?.kind).toBe('pc');
    // read header of a Wi-Fi phone without a game version
    store.read.start(store.devices.find((d) => d.id === 'w')!);
    expect(await screen.findByText('Google Pixel 8 · Wi-Fi · Game 1.7.6')).toBeInTheDocument();
    store.read.start(store.devices.find((d) => d.id === 'ng')!);
    expect(await screen.findByText('Google Pixel 8 · USB')).toBeInTheDocument();
    // transfer hints with unknown values
    store.transfer.start();
    expect(await screen.findByText('Android - · game 1.7.6 · Wi-Fi')).toBeInTheDocument();
    expect(screen.getByText('Android 15 · game - · USB')).toBeInTheDocument();
    expect(screen.getByText('This computer · game -')).toBeInTheDocument();
    store.openGuide('pc');
    expect(await screen.findByText(/0 worlds · The Blockheads -/)).toBeInTheDocument();
    mock.listDevices = async () => {
      throw { code: 'io' };
    };
    await store.refreshDevices();
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    expect(store.scanning).toBe(false);
  });

  it('device guides navigation and empty-state buttons', async () => {
    await setup({ scenario: 'empty', before: (m) => (m.settings.first_run_done = true) });
    await fireEvent.click(await screen.findByRole('button', { name: 'USB cable' }));
    for (const name of ['Windows USB driver', 'Troubleshooting', 'Wi-Fi pairing', 'Rooted Android', 'Blockheads PC', 'iPhone backup', 'USB cable']) {
      await fireEvent.click(within(screen.getByRole('navigation', { name: 'Connection types' })).getByRole('button', { name: new RegExp(name) }));
      expect(screen.getByRole('heading', { level: 2, name })).toBeInTheDocument();
    }
  });

  it('read flow: select all from none, picker cancelled, export detail without an item', async () => {
    const { mock } = await setup();
    mock.picks.folder = null;
    store.read.start(store.devices.find((d) => d.id === 'pc') as Device);
    await fireEvent.click(await screen.findByRole('button', { name: 'Start reading' }));
    await screen.findByRole('table');
    store.read.toggleAll(false);
    store.read.toggleAll(true);
    expect(store.read.selected).toHaveLength(5);
    await fireEvent.click(screen.getByRole('button', { name: /Export 5 worlds/ }));
    await waitFor(() => expect(store.read.free).not.toBeNull());
    expect(screen.getByLabelText('Save to')).toHaveValue('');
    expect(screen.getByRole('button', { name: 'Export' })).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Browse…' }));
    store.read.free = null;
    await waitFor(() => expect(screen.queryByText(/free/)).toBeNull());
    store.read.stage = 'exporting';
    store.read.progress = { stage: 'exporting' };
    await fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    store.read.progress = { stage: 'exporting', item: 'X.bhworld' };
    expect(await screen.findByText('X.bhworld · 0 of 0')).toBeInTheDocument();
  });

  it('transfer: same-world conflicts, nothing added, writing without numbers, cancel on the phone', async () => {
    const { mock } = await setup({ tick: 30 });
    const lib = await mock.listWorlds();
    const sunny = lib.find((w) => w.name === 'Sunny Isle')!;
    mock.phoneWorlds.pixel8 = [{ ...sunny, id: sunny.world_id, name: 'Other name' }];
    store.transfer.start({ to: 'pixel8', worlds: [sunny.id] });
    await waitFor(() => expect(Array.isArray(store.transfer.available)).toBe(true));
    await store.transfer.review();
    expect(await screen.findByText('Same world')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: 'Replace' }));
    expect(screen.queryByText(/Copy will be named/)).toBeNull();
    store.transfer.stage = 'before';
    await fireEvent.click(await screen.findByLabelText('The game is closed'));
    const run = store.transfer.write();
    await screen.findByRole('img', { name: /Full backup/ });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await run;
    expect(store.transfer.stage).toBe('before');
    mock.tick = 0;
    await store.transfer.write();
    expect(await screen.findByText(/Other name replaced|Sunny Isle replaced/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(store.screen).toBe('devices');
    store.transfer.stage = 'running';
    store.transfer.progress = { stage: 'writing' };
    store.screen = 'transfer';
    expect(await screen.findByText(/0 KB|1 KB/)).toBeInTheDocument();
  });

  it('worlds: delete an unselected world, toggle details off, server and unplayed worlds', async () => {
    const { mock } = await setup();
    mock.worlds[0] = { ...mock.worlds[0], mode: 'expert', source_detail: 'server', last_played: null };
    mock.worlds[1] = { ...mock.worlds[1], source_detail: null };
    store.go('worlds');
    await screen.findByRole('list');
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Sunny Isle' }));
    const panel = screen.getByRole('complementary');
    expect(panel).toHaveTextContent('Expert');
    expect(panel).toHaveTextContent('Never');
    expect(within(panel).getByText('Ada’s Pixel 8')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Deep Mine' }));
    expect(within(screen.getByRole('complementary')).getByText('Ada’s Pixel 8')).toBeInTheDocument();
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Edit' }));
    expect(store.screen).toBe('editor');
    store.go('worlds');
    await fireEvent.click(await screen.findByRole('button', { name: 'Details of Deep Mine' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Deep Mine' }));
    expect(screen.queryByRole('complementary')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Deep Mine' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for Castle Build' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText('Castle Build deleted')).toBeInTheDocument();
    expect(screen.getByRole('complementary', { name: 'Details of Deep Mine' })).toBeInTheDocument();
    // progress that arrives after the export dialog closed is ignored
    let late: ((p: { stage: 'exporting' }) => void) | null = null;
    mock.exportWorlds = async (_op, req, onProgress) => {
      late = onProgress as never;
      return { folder: req.folder, files: [{ name: 'a', size: 1 }], saved: [] };
    };
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Export as .bhworld' }));
    await screen.findByText('a exported');
    (late as unknown as (p: { stage: 'exporting' }) => void)({ stage: 'exporting' });
  });

  it('dialog close buttons and menus that toggle', async () => {
    const { mock } = await setup({ before: (m) => (m.update = { version: '0.3.0', notes: [] }) });
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Close' }));
    expect(store.update.popup).toBe(false);
    const more = screen.getByRole('button', { name: 'More actions for Ada’s Pixel 8' });
    await fireEvent.click(more);
    await fireEvent.click(more);
    expect(screen.queryByRole('menu')).toBeNull();
    store.go('worlds');
    await screen.findByRole('list');
    await fireEvent.click(screen.getByRole('button', { name: 'Import world' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Close' }));
    store.go('activity');
    await fireEvent.click(await screen.findByRole('button', { name: 'Clear' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Close' }));
    store.go('backups');
    await fireEvent.click((await screen.findAllByRole('button', { name: 'Restore to device…' }))[0]);
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Close' }));
    await fireEvent.click(screen.getAllByRole('button', { name: /More actions for the backup/ })[0]);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Close' }));
    await fireEvent.click(within(screen.getByRole('group', { name: 'Show backups of' })).getByRole('button', { name: /Galaxy Tab S6 Lite/ }));
    expect(screen.getAllByRole('row')).toHaveLength(2);
    store.go('settings');
    await fireEvent.click(await screen.findByRole('button', { name: 'Manage…' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getAllByRole('button', { name: 'Close' })[0]);
    await fireEvent.click(screen.getByRole('button', { name: 'Clear extracted files' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getAllByRole('button', { name: 'Close' })[0]);
    expect(mock).toBeTruthy();
  });

  it('game file dialog: a drop while it shows an error is used as the APK; copy without detail', async () => {
    const { mock } = await setup({ before: (m) => (m.game.state = 'not_set') });
    mock.fail.apk = { code: 'apk_damaged' };
    store.gameFiles.open();
    await store.gameFiles.useFile('/d/TheBlockheads.apk');
    expect(store.gameFiles.stage).toBe('error');
    await fireEvent.click(screen.getByRole('button', { name: 'Copy details' }));
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('apk_damaged');
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy details' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    mock.drag({ type: 'drop', paths: ['/d/TheBlockheads.apk'] });
    await waitFor(() => expect(store.gameFiles.stage).toBe('ready'));
    mock.drag({ type: 'drop', paths: ['/d/Gift.bhworld'] });
    expect(store.importer.isOpen).toBe(true);
  });

  it('settings: keep count and language selects reflect the current values', async () => {
    await setup({ before: (m) => (m.settings.backups_to_keep = 3) });
    await fireEvent.click(screen.getAllByRole('button', { name: 'Refresh' })[1]);
    store.go('settings');
    store.game = { ...store.game!, state: 'damaged' };
    await fireEvent.click(await screen.findByRole('button', { name: 'Clear extracted files' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Cancel' }));
    expect(((await screen.findByRole('combobox', { name: 'Backups to keep per device' })) as HTMLSelectElement).value).toBe('3');
    expect((screen.getByRole('combobox', { name: 'Language' }) as HTMLSelectElement).value).toBe('English');
    store.settings = { ...store.settings!, backups_to_keep: 10 };
    await waitFor(() => expect((screen.getByRole('combobox', { name: 'Backups to keep per device' }) as HTMLSelectElement).value).toBe('10'));
    const { setLang } = await import('../i18n.svelte');
    setLang('tr');
    await waitFor(() => expect((screen.getByRole('combobox', { name: 'Dil' }) as HTMLSelectElement).value).toBe('Türkçe'));
    setLang('en');
  });

  it('first run language select follows the language; a failed write offers to try again', async () => {
    const { mock } = await setup({ scenario: 'fresh' });
    const { setLang } = await import('../i18n.svelte');
    setLang('kk');
    await waitFor(() => expect((screen.getByRole('combobox') as HTMLSelectElement).value).toBe('Қазақша'));
    setLang('en');
    await store.saveSettings({ first_run_done: true });
    store.transfer.start({ to: 'pc' });
    await waitFor(() => expect(store.transfer.picked.length).toBe(0));
    mock.worlds = (await new (await import('../mock-api')).MockBackend({ tick: 0 }).listWorlds());
    await store.transfer.loadSource();
    await store.transfer.review();
    mock.fail.transfer = { code: 'declined', device: 'Blockheads PC' };
    await store.transfer.write();
    const dlg = await screen.findByRole('dialog', { name: 'The phone said no' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Start again' }));
    expect(await screen.findByRole('heading', { name: /Blockheads PC now has/ })).toBeInTheDocument();
  });
});
