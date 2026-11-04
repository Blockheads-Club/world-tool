import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Backups', () => {
  it('lists, filters, restores with the phone steps, deletes', async () => {
    const { mock } = await setup({ tick: 10 });
    await fireEvent.click(screen.getByRole('button', { name: 'Backups' }));
    await screen.findByRole('table');
    expect(screen.getByText(/3 backups · 621 MB/)).toBeInTheDocument();
    expect(screen.getByText('Before writing 7 worlds')).toBeInTheDocument();
    expect(screen.getByText('Read for export')).toBeInTheDocument();
    expect(screen.getByText('We keep the last 5 backups per device — change it in Settings.')).toBeInTheDocument();
    const chips = screen.getByRole('group', { name: 'Show backups of' });
    await fireEvent.click(within(chips).getByRole('button', { name: /Galaxy Tab/ }));
    expect(screen.getAllByRole('row')).toHaveLength(2);
    await fireEvent.click(within(chips).getByRole('button', { name: /All devices/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Open folder' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Restore to device…' })[0]);
    let dlg = await screen.findByRole('dialog', { name: 'Restore a backup' });
    expect(within(dlg).getByRole('combobox', { name: 'Restore to' })).toHaveValue('Ada’s Pixel 8 · USB · Game 1.7.6');
    const go = within(dlg).getByRole('button', { name: 'Restore' });
    expect(go).toBeDisabled();
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Restore to device…' })[0]);
    dlg = await screen.findByRole('dialog', { name: 'Restore a backup' });
    await fireEvent.click(within(dlg).getByLabelText('The game is closed on the phone'));
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Restore' }));
    expect(await screen.findByText('First the current game data is saved as a new backup, so you can still go back.')).toBeInTheDocument();
    expect(await screen.findByText('Look at your phone and tap “Restore my data”')).toBeInTheDocument();
    expect(await screen.findByRole('progressbar', { name: 'Restoring the backup' })).toBeInTheDocument();
    expect(await screen.findByText('Tap “Back up my data” once more')).toBeInTheDocument();
    expect(await screen.findByText('Backup restored to Ada’s Pixel 8', {}, { timeout: 2000 })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/4 backups/)).toBeInTheDocument());
    // delete via the More menu
    await fireEvent.click(screen.getAllByRole('button', { name: /More actions for the backup of Galaxy Tab/ })[0]);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Show in folder' }));
    await fireEvent.click(screen.getAllByRole('button', { name: /More actions for the backup of Galaxy Tab/ })[0]);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    let del = await screen.findByRole('alertdialog', { name: 'Delete this backup?' });
    await fireEvent.click(within(del).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getAllByRole('button', { name: /More actions for the backup of Galaxy Tab/ })[0]);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    del = await screen.findByRole('alertdialog');
    await fireEvent.click(within(del).getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(screen.queryByRole('button', { name: /Galaxy Tab/ })).toBeNull());
    // a failed restore shows the error and keeps the dialog
    mock.fail.restore = { code: 'declined', device: 'Ada’s Pixel 8' };
    mock.tick = 0;
    await fireEvent.click(screen.getAllByRole('button', { name: 'Restore to device…' })[0]);
    dlg = await screen.findByRole('dialog', { name: 'Restore a backup' });
    await fireEvent.click(within(dlg).getByLabelText('The game is closed on the phone'));
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Restore' }));
    const err = await screen.findByRole('dialog', { name: 'The phone said no' });
    await fireEvent.click(within(err).getByRole('button', { name: 'Start again' }));
    expect(await screen.findByText('Backup restored to Ada’s Pixel 8')).toBeInTheDocument();
  });

  it('restores to Blockheads PC without the checkbox; cancel while waiting; no target; empty', async () => {
    const { mock } = await setup({ tick: 30, before: (m) => {
      m.backups.push({ id: 'pcb', device_id: 'pc', device: 'Blockheads PC', reason: 'before_restore', reason_count: null, time: Date.now(), worlds: 5, size: 1, automatic: false });
    } });
    store.restoreId = 'pcb';
    store.go('backups');
    const dlg = await screen.findByRole('dialog', { name: 'Restore a backup' });
    expect(screen.getByText('Before restoring a backup')).toBeInTheDocument();
    expect(screen.getByText('Manual')).toBeInTheDocument();
    expect(within(dlg).queryByLabelText('The game is closed on the phone')).toBeNull();
    await fireEvent.change(within(dlg).getByRole('combobox'), { target: { selectedIndex: 1 } });
    await fireEvent.click(within(dlg).getByLabelText('The game is closed on the phone'));
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Restore' }));
    await screen.findByText('Look at your phone and tap “Back up my data”');
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Restore' })).toBeInTheDocument(), { timeout: 2000 });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    store.devices = [];
    store.restoreId = 'missing';
    store.go('devices');
    store.go('backups');
    await screen.findByRole('table');
    expect(screen.queryByRole('dialog')).toBeNull();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Restore to device…' })[0]);
    expect(await screen.findByText('Connect the device first.')).toBeInTheDocument();
    mock.backups = [];
    store.go('devices');
    await screen.findByRole('heading', { name: 'Devices', level: 1 });
    store.go('backups');
    expect(await screen.findByText('No backups yet')).toBeInTheDocument();
  });
});
