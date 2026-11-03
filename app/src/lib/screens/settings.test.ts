import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '../i18n.svelte';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Settings', () => {
  it('changes every setting', async () => {
    const { mock } = await setup();
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await screen.findByRole('heading', { name: 'Settings', level: 1 });
    expect(screen.getByText(/The Blockheads 1\.7\.6 · TheBlockheads\.apk · 1084 images · 38 MB/)).toBeInTheDocument();
    mock.picks.folder = null;
    await fireEvent.click(screen.getAllByRole('button', { name: 'Change…' })[0]);
    mock.picks.folder = '/new/worlds';
    await fireEvent.click(screen.getAllByRole('button', { name: 'Change…' })[0]);
    await waitFor(() => expect(screen.getByLabelText('Worlds folder')).toHaveValue('/new/worlds'));
    await fireEvent.click(screen.getByRole('switch', { name: 'Back up before every write' }));
    await waitFor(() => expect(store.settings?.backup_before_write).toBe(false));
    await fireEvent.change(screen.getByRole('combobox', { name: 'Backups to keep per device' }), { target: { value: '10' } });
    await waitFor(() => expect(store.settings?.backups_to_keep).toBe(10));
    await fireEvent.click(screen.getByRole('radio', { name: 'Always' }));
    await fireEvent.click(screen.getByRole('radio', { name: 'Dark' }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('dark'));
    await fireEvent.click(screen.getByRole('switch', { name: 'Reduce motion' }));
    await waitFor(() => expect(document.documentElement.dataset.motion).toBe('reduce'));
    await fireEvent.click(screen.getByRole('switch', { name: 'Check for updates automatically' }));
    await fireEvent.change(screen.getByRole('combobox', { name: 'Language' }), { target: { selectedIndex: 1 } });
    await waitFor(() => expect(i18n.lang).toBe('tr'));
    await fireEvent.change(screen.getByRole('combobox', { name: 'Dil' }), { target: { selectedIndex: 0 } });
    await waitFor(() => expect(i18n.lang).toBe('en'));
    expect(screen.getByText('2 devices')).toBeInTheDocument();
    // paired devices
    await fireEvent.click(screen.getByRole('button', { name: 'Manage…' }));
    let dlg = await screen.findByRole('dialog', { name: 'Paired Wi-Fi devices' });
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Forget' })[0]);
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Forget' })[0]);
    expect(await within(dlg).findByText('No phones are paired over Wi-Fi.')).toBeInTheDocument();
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Done' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Manage…' }));
    dlg = await screen.findByRole('dialog');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Pair a phone' }));
    expect(store.guide).toBe('wifi');
    store.go('settings');
    // about
    await fireEvent.click(await screen.findByRole('button', { name: 'View' }));
    dlg = await screen.findByRole('dialog', { name: 'Open-source licences' });
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Close' })[1]);
    await fireEvent.click(screen.getByRole('button', { name: 'Read' }));
    dlg = await screen.findByRole('dialog', { name: 'Legal notice' });
    await fireEvent.keyDown(dlg, { key: 'Escape' });
    await fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Copied to the clipboard')).toBeInTheDocument();
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    // updates
    await fireEvent.click(screen.getByRole('button', { name: 'Check now' }));
    expect(await screen.findByText('You have the latest version')).toBeInTheDocument();
    store.settings!.last_update_check = null;
    expect(await screen.findByText('Not checked yet')).toBeInTheDocument();
  });

  it('game files: ready (replace, re-extract, clear), not set, unsupported, damaged, missing', async () => {
    const { mock } = await setup();
    store.go('settings');
    await screen.findByText('Ready');
    await fireEvent.click(screen.getByRole('button', { name: 'Replace APK…' }));
    expect(store.gameFiles.isOpen).toBe(true);
    store.gameFiles.close();
    await fireEvent.click(screen.getByRole('button', { name: 'Re-extract' }));
    await screen.findByRole('dialog', { name: 'Ready to edit' });
    store.gameFiles.close();
    await fireEvent.click(screen.getByRole('button', { name: 'Clear extracted files' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Clear extracted files' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog', { name: 'Clear the extracted graphics?' })).getByRole('button', { name: 'Clear extracted files' }));
    expect(await screen.findByText('Not set')).toBeInTheDocument();
    expect(screen.getByText(/Needed only to view and edit worlds/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Choose APK…' }));
    store.gameFiles.close();
    await fireEvent.click(screen.getByRole('button', { name: 'Get it from a phone' }));
    await screen.findByRole('dialog', { name: 'Ready to edit' });
    store.gameFiles.close();
    store.game = { ...store.game!, state: 'unsupported', apk_name: 'TheBlockheads.apk', version: '1.6.3' };
    expect(await screen.findByText('Unsupported')).toBeInTheDocument();
    expect(screen.getByText(/TheBlockheads\.apk is version 1\.6\.3/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Replace APK…' }));
    store.gameFiles.close();
    store.game = { ...store.game!, state: 'unsupported', apk_name: null, version: null };
    store.game = { ...store.game!, state: 'damaged' };
    expect(await screen.findByText('Damaged')).toBeInTheDocument();
    store.game = { ...store.game!, state: 'missing' };
    expect(await screen.findByText('File missing')).toBeInTheDocument();
    store.game = { ...store.game!, state: 'ready', version: null, apk_name: null, images: null, size: null };
    await screen.findByText('Ready');
    store.game = null;
    expect(await screen.findByText('Not set')).toBeInTheDocument();
    store.devices = [];
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Get it from a phone' })).toBeNull());
    expect(mock).toBeTruthy();
  });

  it('shows the update available notice', async () => {
    await setup({ before: (m) => (m.update = { version: '0.3.0', notes: ['Faster reading over Wi-Fi.'] }) });
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'BH World Tool 0.3.0 is available' })).getByRole('button', { name: 'Later' }));
    store.go('settings');
    expect(await screen.findByText('Faster reading over Wi-Fi.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'What’s new' }));
    expect(await screen.findByRole('dialog', { name: 'BH World Tool 0.3.0 is available' })).toBeInTheDocument();
    store.update.popup = false;
    store.update.info = { ...store.update.info!, notes: [] };
    await fireEvent.click(await screen.findByRole('button', { name: 'Update and restart' }));
    await waitFor(() => expect(store.update.stage).toBe('ready'));
  });
});
