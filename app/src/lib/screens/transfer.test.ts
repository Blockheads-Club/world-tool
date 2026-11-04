import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Transfer', () => {
  it('copies from Worlds to a phone: choose → review → before → phone → writing → done', async () => {
    const { mock } = await setup({ tick: 10 });
    await fireEvent.click(screen.getByRole('button', { name: 'Transfer' }));
    await screen.findByRole('heading', { name: 'Transfer', level: 1 });
    await waitFor(() => expect(screen.getByText('7 worlds on this computer')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Next: review' })).toBeDisabled();
    // iOS can't be a target
    const iosRow = screen.getAllByRole('radio', { name: /Ada’s iPhone backup/ }).find((r) => r.getAttribute('aria-disabled') === 'true') as HTMLElement;
    await fireEvent.keyDown(iosRow, { key: 'Enter' });
    expect(screen.queryByText('iPhone and iPad can’t be written')).toBeNull();
    await fireEvent.keyDown(iosRow, { key: ' ' });
    expect(screen.getByText('iPhone and iPad can’t be written')).toBeInTheDocument();
    await fireEvent.click(iosRow);
    const to = screen.getByRole('radiogroup', { name: 'To' });
    expect(within(to).getByLabelText(/moto g\(7\)/)).toBeDisabled();
    await fireEvent.click(within(to).getByLabelText(/Ada’s Pixel 8/));
    // pick worlds: Sunny Isle (conflict) + Pirate Cove (new)
    await fireEvent.click(screen.getByRole('button', { name: 'Change…' }));
    const pick = await screen.findByRole('dialog', { name: 'Choose worlds' });
    for (const cb of within(pick).getAllByRole('checkbox')) if ((cb as HTMLInputElement).checked) await fireEvent.click(cb);
    await fireEvent.click(within(pick).getByLabelText('Sunny Isle'));
    await fireEvent.click(within(pick).getByLabelText('Pirate Cove'));
    await fireEvent.click(within(pick).getByRole('button', { name: 'Done' }));
    expect(screen.getByText('Pirate Cove')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Next: review' }));
    await screen.findByRole('heading', { name: 'After copying, Ada’s Pixel 8 will have 7 worlds' });
    expect(screen.getByText('Copy will be named “Sunny Isle (2)”')).toBeInTheDocument();
    const seg = screen.getByRole('radiogroup', { name: 'Sunny Isle: what to do' });
    await fireEvent.keyDown(within(seg).getByRole('radio', { name: 'Keep both' }), { key: 'ArrowRight' });
    expect(within(seg).getByRole('radio', { name: 'Replace' })).toHaveAttribute('aria-checked', 'true');
    await fireEvent.keyDown(within(seg).getByRole('radio', { name: 'Replace' }), { key: 'ArrowUp' });
    await fireEvent.keyDown(within(seg).getByRole('radio', { name: 'Keep both' }), { key: 'ArrowLeft' });
    expect(within(seg).getByRole('radio', { name: 'Skip' })).toHaveAttribute('aria-checked', 'true');
    await fireEvent.keyDown(within(seg).getByRole('radio', { name: 'Skip' }), { key: 'ArrowDown' });
    await fireEvent.keyDown(within(seg).getByRole('radio', { name: 'Keep both' }), { key: 'Tab' });
    await fireEvent.click(within(seg).getByRole('radio', { name: 'Skip' }));
    expect(screen.getByText('1 · 0 replaced · 1 skipped')).toBeInTheDocument();
    await fireEvent.click(within(seg).getByRole('radio', { name: 'Keep both' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Next: review' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Next' }));
    // before writing
    expect(await screen.findByText('Writing replaces the game’s saved data on the phone')).toBeInTheDocument();
    expect(screen.getByText('Automatic backup first')).toBeInTheDocument();
    const go = screen.getByRole('button', { name: 'Back up and write' });
    expect(go).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Next' }));
    await fireEvent.click(await screen.findByLabelText('The game is closed'));
    mock.tick = 25;
    await fireEvent.click(screen.getByRole('button', { name: 'Back up and write' }));
    expect(await screen.findByText('Look at your phone and tap “Back up my data”')).toBeInTheDocument();
    expect(await screen.findByText('Look at your phone and tap “Restore my data”', {}, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByText('Backup saved · Backups')).toBeInTheDocument();
    expect(await screen.findByRole('progressbar', { name: /Writing 7 worlds to Ada’s Pixel 8/ }, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByText('Check the worlds are on the phone')).toBeInTheDocument();
    expect(await screen.findByText('Tap “Back up my data” once more', {}, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByText('Worlds sent to the phone')).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'Ada’s Pixel 8 now has 7 worlds' }, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByText(/Pirate Cove added/)).toBeInTheDocument();
    expect(screen.getByText(/Sunny Isle \(2\) was added next to the original/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Restore backup…' }));
    expect(store.screen).toBe('backups');
    expect(await screen.findByRole('dialog', { name: 'Restore a backup' })).toBeInTheDocument();
  });

  it('writes to Blockheads PC from files, with backups off; replace and show activity', async () => {
    const { mock } = await setup({ before: (m) => (m.settings.backup_before_write = false) });
    store.transfer.start({ to: 'pc' });
    await screen.findByRole('heading', { name: 'Transfer', level: 1 });
    mock.picks.files = [];
    await fireEvent.click(screen.getByLabelText(/\.bhworld file or server folder/));
    expect(store.transfer.from).toBe('library');
    mock.picks.files = ['/d/Skyblock.bhworld', '/d/Frozen.bhworld'];
    await fireEvent.click(screen.getByLabelText(/\.bhworld file or server folder/));
    await waitFor(() => expect(screen.getByText('2 files chosen')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Next: review' }));
    await screen.findByRole('heading', { name: /Blockheads PC will have 7 worlds/ });
    expect(screen.getAllByText('Same name').length).toBeGreaterThan(0);
    await fireEvent.click(screen.getByRole('radio', { name: 'Replace' }));
    await screen.findByRole('heading', { name: /Blockheads PC will have 6 worlds/ });
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(await screen.findByText('Writing changes Blockheads PC’s saved worlds')).toBeInTheDocument();
    expect(screen.getByText('Automatic backups are off')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Write' }));
    expect(await screen.findByRole('heading', { name: 'Blockheads PC now has 6 worlds' })).toBeInTheDocument();
    expect(screen.getByText(/Skyblock replaced/)).toBeInTheDocument();
    expect(screen.getByText(/Open Blockheads PC to check/)).toBeInTheDocument();
    expect(screen.queryByText('Something wrong?')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Show activity' }));
    expect(store.screen).toBe('activity');
  });

  it('from a phone that was not read yet; from Blockheads PC; source is not a target', async () => {
    await setup();
    store.transfer.start();
    await screen.findByRole('heading', { name: 'Transfer', level: 1 });
    await fireEvent.click(screen.getByLabelText(/Galaxy Tab S6 Lite.*Read worlds first/s));
    expect(await screen.findByText('Read the worlds of this phone first')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Read worlds' }));
    expect(store.screen).toBe('read');
    expect(store.read.device?.id).toBe('tab');
    store.transfer.start({ from: 'pc', to: 'pc' });
    await screen.findByText('This is the source');
    await fireEvent.click(screen.getByLabelText(/Ada’s iPhone backup.*Read-only source/s));
    await waitFor(() => expect(store.transfer.available).not.toBeNull());
    expect(screen.getByText('Pirate Bay')).toBeInTheDocument();
    const from = screen.getByRole('radiogroup', { name: 'From' });
    await fireEvent.click(within(from).getByLabelText(/^Blockheads PC/));
    await fireEvent.click(within(from).getByLabelText(/^Worlds/));
    await waitFor(() => expect(store.transfer.from).toBe('library'));
  });

  it('shows the source error, an empty pick list, and the write error with undo', async () => {
    const { mock } = await setup({ tick: 0 });
    mock.fail.source = { code: 'io', detail: 'x' };
    store.transfer.start({ to: 'pixel8' });
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    expect(screen.getByText('none chosen')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Change…' }));
    expect(await screen.findByText('This source has no worlds.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    // plan error (read-only target forced)
    store.transfer.to = 'iphone';
    await store.transfer.loadSource();
    await store.transfer.review();
    expect(store.transfer.stage).toBe('choose');
    store.transfer.to = 'pixel8';
    await store.transfer.review();
    store.transfer.stage = 'before';
    store.transfer.gameClosed = true;
    mock.fail.transfer = { code: 'disconnected', percent: 61 };
    await fireEvent.click(await screen.findByRole('button', { name: 'Back up and write' }));
    const dlg = await screen.findByRole('alertdialog', { name: 'The device disconnected' });
    expect(dlg).toHaveTextContent('Ada’s Pixel 8 disconnected at 61 %');
    expect(dlg).toHaveTextContent('Your backup is safe');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Restore backup…' }));
    expect(store.screen).toBe('backups');
  });

  it('reports a declined restore and tries again', async () => {
    const { mock } = await setup({ tick: 0 });
    store.transfer.start({ to: 'pixel8' });
    await waitFor(() => expect(store.transfer.picked.length).toBe(2));
    await store.transfer.review();
    store.transfer.stage = 'before';
    store.transfer.gameClosed = true;
    mock.fail.verify = { code: 'restore_declined', detail: 'world x is missing' };
    await fireEvent.click(await screen.findByRole('button', { name: 'Back up and write' }));
    const dlg = await screen.findByRole('dialog', { name: 'The worlds are not on Ada’s Pixel 8' });
    expect(dlg).toHaveTextContent('“Restore my data” was probably not tapped');
    expect(dlg).toHaveTextContent('Nothing was lost');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: /now has/ }, { timeout: 3000 })).toBeInTheDocument();
    // Without a backup there is no "nothing was lost" note.
    store.shown = { error: { code: 'restore_declined' } };
    const plain = await screen.findByRole('dialog', { name: 'The worlds are not on the phone' });
    expect(plain).not.toHaveTextContent('Nothing was lost');
  });

  it('cancels while running and shows the running steps', async () => {
    await setup({ tick: 30, before: (m) => (m.settings.root_mode = 'auto') });
    store.transfer.start({ to: 'redmi' });
    await waitFor(() => expect(store.transfer.picked.length).toBe(2));
    await store.transfer.review();
    store.transfer.stage = 'before';
    await fireEvent.click(await screen.findByRole('button', { name: 'Back up and write' }));
    expect(await screen.findByText('Backup saved')).toBeInTheDocument();
    expect(await screen.findByText('Worlds prepared', {}, { timeout: 2000 })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await screen.findByRole('button', { name: 'Back up and write' }, { timeout: 2000 });
  });
});
