import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Phone tool (adb) download', () => {
  it('offers the download when adb is missing, shows source and size, downloads', async () => {
    const { mock } = await setup({ noAdb: true, tick: 5 });
    expect(store.devices.some((d) => d.kind === 'android')).toBe(false);
    await screen.findByText('Phone tool needed');
    await fireEvent.click(screen.getByRole('button', { name: 'Get adb' }));
    const dlg = await screen.findByRole('dialog', { name: 'Download the phone connection tool' });
    await within(dlg).findByText('https://dl.google.com/android/repository/platform-tools-latest-linux.zip');
    expect(dlg).toHaveTextContent('7 MB');
    expect(dlg).toHaveTextContent('developer.android.com');
    expect(dlg).toHaveTextContent('Already have adb?');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Download' }));
    expect(await screen.findByRole('progressbar', { name: 'Downloading from Google' })).toBeInTheDocument();
    await screen.findByRole('dialog', { name: 'Phone tool ready' }, { timeout: 3000 });
    expect(screen.getByText(/platform-tools\/adb/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(store.adb.isOpen).toBe(false);
    await waitFor(() => expect(store.devices.some((d) => d.kind === 'android')).toBe(true));
    expect(screen.queryByText('Phone tool needed')).toBeNull();
    expect(mock.adb.managed).toBe(true);
    // Settings shows where it is.
    store.go('settings');
    expect(await screen.findByText(/Downloaded from Google: .*platform-tools\/adb/)).toBeInTheDocument();
    expect(screen.getByText('Installed')).toBeInTheDocument();
  });

  it('shows failures with retry, and cancel', async () => {
    const { mock } = await setup({ noAdb: true, tick: 0 });
    // Asking for the size fails, then works.
    mock.fail.adb = { code: 'download_failed', detail: 'offline' };
    await store.adb.open();
    let dlg = await screen.findByRole('alertdialog', { name: 'The download didn’t work' });
    expect(dlg).toHaveTextContent('Check the internet connection');
    expect(dlg).toHaveTextContent('offline');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Try again' }));
    // The download fails, then works.
    const download = await screen.findByRole('button', { name: 'Download' });
    mock.fail.adb = { code: 'download_failed' };
    await fireEvent.click(download);
    dlg = await screen.findByRole('alertdialog', { name: 'The download didn’t work' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Try again' }));
    await screen.findByRole('dialog', { name: 'Phone tool ready' });
    // Escape closes.
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(store.adb.isOpen).toBe(false);
    await store.adb.open();
    // Not now closes.
    mock.adb.found = false;
    await store.adb.open();
    await fireEvent.click(await screen.findByRole('button', { name: 'Not now' }));
    expect(store.adb.isOpen).toBe(false);
    // Cancel while downloading goes back to the offer.
    mock.tick = 20;
    await store.adb.open();
    void store.adb.download();
    await screen.findByRole('progressbar', { name: 'Downloading from Google' });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(store.adb.isOpen).toBe(false);
    await waitFor(() => expect(store.adb.stage).toBe('offer'));
  });

  it('opens from an adb_missing error and from Settings; progress steps', async () => {
    const { mock } = await setup({ noAdb: true, tick: 0 });
    store.fail({ code: 'adb_missing' });
    await screen.findByRole('dialog', { name: 'Download the phone connection tool' });
    store.adb.close();
    store.go('settings');
    expect(await screen.findByText('Not installed. Needed for Android phones.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Get adb' }));
    await screen.findByRole('button', { name: 'Download' });
    // Unpacking / checking / unknown size labels.
    store.adb.stage = 'downloading';
    store.adb.progress = { stage: 'preparing' };
    expect(await screen.findByRole('progressbar', { name: 'Unpacking' })).toBeInTheDocument();
    store.adb.progress = { stage: 'checking' };
    expect(await screen.findByRole('progressbar', { name: 'Checking that adb runs' })).toBeInTheDocument();
    store.adb.progress = { stage: 'downloading', done: 10 };
    expect(await screen.findByText(/ of \?$/)).toBeInTheDocument();
    store.adb.stage = 'offer';
    store.adb.info = { ...store.adb.info!, size: null };
    expect(await screen.findByText('Unknown')).toBeInTheDocument();
    store.adb.stage = 'error';
    store.adb.error = { code: 'io' };
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    store.adb.close();
    // Found adb: Settings shows the path.
    mock.adb = { ...mock.adb, found: true, path: '/usr/bin/adb' };
    await store.adb.load();
    expect(await screen.findByText('Found: /usr/bin/adb')).toBeInTheDocument();
  });
});
