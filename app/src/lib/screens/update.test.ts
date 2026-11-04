import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());
const offer = { version: '0.3.0', notes: ['Faster reading over Wi-Fi.', 'Fixes for Samsung phones.'] };

describe('Updates', () => {
  it('popup → later → chip → download → ready → restart', async () => {
    const { mock } = await setup({ tick: 10, before: (m) => (m.update = offer) });
    const dlg = await screen.findByRole('dialog', { name: 'BH World Tool 0.3.0 is available' });
    expect(dlg).toHaveTextContent('Fixes for Samsung phones.');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Later' }));
    const bar = screen.getByRole('banner', { name: 'Window title bar' });
    const chip = within(bar).getByRole('button', { name: 'Update available: version 0.3.0. Open details.' });
    await fireEvent.click(chip);
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Update and restart' }));
    expect(await within(bar).findByRole('button', { name: /Downloading update, \d+ percent/ })).toBeInTheDocument();
    await fireEvent.click(within(bar).getByRole('button', { name: /Downloading update/ }));
    const dl = await screen.findByRole('dialog', { name: 'BH World Tool 0.3.0 is available' });
    expect(within(dl).getByRole('progressbar', { name: 'Downloading the update' })).toBeInTheDocument();
    await fireEvent.click(within(dl).getByRole('button', { name: 'Hide' }));
    expect(await screen.findByText('Update 0.3.0 is ready', {}, { timeout: 2000 })).toBeInTheDocument();
    const restart = vi.spyOn(mock, 'restartApp');
    await fireEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(screen.queryByText('Update 0.3.0 is ready')).toBeNull();
    await fireEvent.click(within(bar).getByRole('button', { name: 'Update ready. Restart to finish.' }));
    expect(restart).toHaveBeenCalledTimes(1);
  });

  it('an update download can be cancelled; the update stays offered', async () => {
    const { mock } = await setup({ tick: 50, before: (m) => (m.update = offer) });
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Update and restart' }));
    const bar = screen.getByRole('banner', { name: 'Window title bar' });
    await fireEvent.click(await within(bar).findByRole('button', { name: /Downloading update/ }));
    const cancel = vi.spyOn(mock, 'cancel');
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel download' }));
    expect(cancel).toHaveBeenCalledWith(expect.stringMatching(/^update/));
    await waitFor(() => expect(store.update.stage).toBe('available'));
    expect(store.update.popover).toBe(false);
    expect(within(bar).getByRole('button', { name: 'Update available: version 0.3.0. Open details.' })).toBeInTheDocument();
  });

  it('skip this version; the next automatic check stays quiet', async () => {
    const { mock } = await setup({ before: (m) => (m.update = offer) });
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Skip this version' }));
    await waitFor(() => expect(mock.settings.skipped_version).toBe('0.3.0'));
    await store.update.check(true);
    expect(store.update.stage).toBe('latest');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('latest / check failed popovers and the Updates chip', async () => {
    const { mock } = await setup();
    await store.update.check();
    const pop = await screen.findByRole('dialog', { name: 'Updates' });
    expect(pop).toHaveTextContent('You have the latest version');
    const chip = within(screen.getByRole('banner', { name: 'Window title bar' })).getByRole('button', { name: 'Updates' });
    expect(chip).toHaveAttribute('aria-expanded', 'true');
    await fireEvent.click(chip);
    expect(screen.queryByRole('dialog', { name: 'Updates' })).toBeNull();
    store.update.popover = true;
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'Updates' })).getByRole('button', { name: 'Update settings' }));
    expect(store.screen).toBe('settings');
    mock.fail.check = { code: 'offline' };
    await store.update.check();
    const failed = await screen.findByRole('dialog', { name: 'Updates' });
    expect(failed).toHaveTextContent('Couldn’t check for updates');
    await fireEvent.click(within(failed).getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(store.update.stage).toBe('latest'));
    await fireEvent.click(within(screen.getByRole('dialog', { name: 'Updates' })).getByRole('button', { name: 'Check again' }));
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'Updates' })).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('banner', { name: 'Window title bar' })?.querySelector('.chip')).toBeNull();
    // a failed download
    mock.fail.update = { code: 'offline' };
    store.update.info = { version: '0.3.0', date: 0, size: 1, notes: [] };
    await store.update.download();
    expect(store.update.stage).toBe('failed');
    // the automatic check stays quiet on errors
    mock.fail.check = { code: 'offline' };
    store.update.popover = false;
    await store.update.check(true);
    expect(store.update.popover).toBe(false);
  });

  it('waits for a running transfer before restarting', async () => {
    const { mock } = await setup({ tick: 20 });
    const restart = vi.spyOn(mock, 'restartApp');
    store.update.stage = 'ready';
    store.update.info = { version: '0.3.0', date: 0, size: 1, notes: [] };
    store.transfer.start({ to: 'pc' });
    await waitFor(() => expect(store.transfer.picked.length).toBe(2));
    await store.transfer.review();
    const run = store.transfer.write();
    await waitFor(() => expect(store.transfer.running).toBe(true));
    await fireEvent.click(screen.getByRole('button', { name: 'Restart now' }));
    let dlg = await screen.findByRole('dialog', { name: 'Restart after the transfer?' });
    expect(dlg).toHaveTextContent('A transfer to Blockheads PC is running');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Not now' }));
    await fireEvent.click(within(screen.getByRole('banner', { name: 'Window title bar' })).getByRole('button', { name: 'Update ready. Restart to finish.' }));
    dlg = await screen.findByRole('dialog', { name: 'Restart after the transfer?' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Close' }));
    await store.update.restart();
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Restart when done' }));
    expect(screen.queryByText('Update 0.3.0 is ready')).toBeNull();
    expect(restart).not.toHaveBeenCalled();
    await run;
    expect(restart).toHaveBeenCalledTimes(1);
    // reading also counts as busy
    store.read.start(store.devices[0]);
    store.screen = 'devices';
    store.read.stage = 'reading';
    store.read.device = null;
    store.transfer.plan = null;
    await store.update.restart();
    expect(await screen.findByRole('dialog', { name: 'Restart after the transfer?' })).toHaveTextContent(/is running \(0 %\)/);
  });
});
