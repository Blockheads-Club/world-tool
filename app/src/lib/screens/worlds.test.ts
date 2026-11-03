import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

async function open() {
  await fireEvent.click(screen.getByRole('button', { name: 'Worlds' }));
  await screen.findByRole('list');
}

describe('Worlds', () => {
  it('shows the loading skeleton, then cards with filters, search and sort', async () => {
    await setup();
    store.screen = 'worlds';
    expect(await screen.findByText('Reading your worlds…')).toBeInTheDocument();
    await screen.findByRole('list');
    expect(screen.getByText(/7 worlds · 327 MB/)).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(7);
    const chips = screen.getByRole('group', { name: 'Show worlds from' });
    await fireEvent.click(within(chips).getByRole('button', { name: /Phones/ }));
    expect(screen.getAllByRole('listitem')).toHaveLength(4);
    await fireEvent.click(within(chips).getByRole('button', { name: /Files/ }));
    expect(screen.getByText('File · Pirate Cove.bhworld')).toBeInTheDocument();
    await fireEvent.click(within(chips).getByRole('button', { name: /iPhone backups/ }));
    await fireEvent.click(within(chips).getByRole('button', { name: /Blockheads PC/ }));
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
    await fireEvent.click(within(chips).getByRole('button', { name: /All/ }));
    await fireEvent.input(screen.getByRole('searchbox'), { target: { value: 'zzz' } });
    expect(screen.getByText('No world matches.')).toBeInTheDocument();
    await fireEvent.input(screen.getByRole('searchbox'), { target: { value: '' } });
    const sort = screen.getByRole('combobox', { name: 'Sort' });
    const first = () => screen.getAllByRole('listitem')[0].getAttribute('aria-label');
    await fireEvent.change(sort, { target: { value: 'name' } });
    expect(first()).toBe('Castle Build');
    await fireEvent.change(sort, { target: { value: 'size' } });
    expect(first()).toBe('Deep Mine');
    await fireEvent.change(sort, { target: { value: 'old' } });
    expect(first()).toBe('Desert Run');
    await fireEvent.click(screen.getByRole('button', { name: 'Open folder' }));
  });

  it('shows details and runs every world action', async () => {
    const { mock } = await setup();
    await open();
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Sunny Isle' }));
    const panel = screen.getByRole('complementary', { name: 'Details of Sunny Isle' });
    expect(panel).toHaveTextContent('Ada’s Pixel 8 · USB');
    expect(panel).toHaveTextContent('World ID');
    // rename
    await fireEvent.click(within(panel).getByRole('button', { name: 'Rename' }));
    const rn = await screen.findByRole('dialog', { name: 'Rename world' });
    await fireEvent.input(within(rn).getByLabelText('New name'), { target: { value: 'Sunny Isle 2' } });
    await fireEvent.submit(within(rn).getByLabelText('New name').closest('form') as HTMLFormElement);
    await screen.findByRole('complementary', { name: 'Details of Sunny Isle 2' });
    await fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Rename' }));
    // duplicate + show in folder
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Duplicate' }));
    expect(await screen.findByText('Sunny Isle 2 (2) added')).toBeInTheDocument();
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Show in folder' }));
    // export as .bhworld (cancelled picker, then done) and as server folder
    mock.picks.folder = null;
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Export as .bhworld' }));
    mock.picks.folder = '/out';
    mock.tick = 10;
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Export as .bhworld' }));
    expect(await screen.findByRole('dialog', { name: 'Exporting Sunny Isle 2' })).toBeInTheDocument();
    expect(await screen.findByText('Sunny Isle 2.bhworld exported')).toBeInTheDocument();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Show in folder' }).at(-1) as HTMLElement);
    mock.tick = 0;
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Export as server folder' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    // copy to a device
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Copy to a device…' }));
    expect(store.screen).toBe('transfer');
    store.go('worlds');
    await screen.findByRole('list');
    await fireEvent.click(screen.getByRole('button', { name: 'Details of Sunny Isle 2' }));
    // delete
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Delete' }));
    const del = await screen.findByRole('alertdialog', { name: 'Delete Sunny Isle 2?' });
    await fireEvent.click(within(del).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Delete' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }));
    expect(await screen.findByText('Sunny Isle 2 deleted')).toBeInTheDocument();
    expect(screen.queryByRole('complementary')).toBeNull();
  });

  it('card buttons and the actions menu', async () => {
    const { mock } = await setup();
    await open();
    const card = screen.getByRole('listitem', { name: 'Deep Mine' });
    await fireEvent.click(within(card).getByRole('button', { name: 'Copy Deep Mine to a device' }));
    expect(store.transfer.picked).toEqual([store.worlds![1].id]);
    store.go('worlds');
    await screen.findByRole('list');
    const menu = async (item: string) => {
      await fireEvent.click(screen.getByRole('button', { name: 'Actions for Castle Build' }));
      await fireEvent.click(screen.getByRole('menuitem', { name: item }));
    };
    await menu('Details');
    expect(screen.getByRole('complementary', { name: 'Details of Castle Build' })).toBeInTheDocument();
    await fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Close' }));
    await menu('Rename');
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Close' }));
    await menu('Duplicate');
    mock.picks.folder = null;
    await menu('Export as .bhworld');
    await menu('Export as server folder');
    await menu('Delete');
    await fireEvent.keyDown(await screen.findByRole('alertdialog'), { key: 'Escape' });
    // a damaged world: export fails → remove from Worlds
    mock.picks.folder = '/out';
    mock.fail.export = { code: 'world_damaged', name: 'Castle Build.bhworld', detail: 'E_WORLD_CORRUPT · chunk 18/42' };
    await menu('Export as .bhworld');
    const dmg = await screen.findByRole('alertdialog', { name: 'Castle Build.bhworld is damaged' });
    expect(dmg).toHaveTextContent('E_WORLD_CORRUPT');
    await fireEvent.click(within(dmg).getByRole('button', { name: 'Copy details' }));
    await fireEvent.click(within(dmg).getByRole('button', { name: 'Remove from Worlds' }));
    await waitFor(() => expect(screen.queryByRole('listitem', { name: 'Castle Build' })).toBeNull());
    // another export error retries the export
    mock.fail.export = { code: 'disk_full', needed: 1, free: 0, disk: '/' };
    await fireEvent.click(screen.getByRole('button', { name: 'Actions for Deep Mine' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Export as server folder' }));
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'Not enough disk space' })).getByRole('button', { name: 'Choose another folder…' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('empty Worlds, the edited notice, and a failed load', async () => {
    const { mock } = await setup({ scenario: 'fresh', before: (m) => (m.settings.first_run_done = true) });
    await fireEvent.click(screen.getByRole('button', { name: 'Worlds' }));
    expect(await screen.findByText('No worlds yet')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Read from a device' }));
    expect(store.screen).toBe('devices');
    store.go('worlds');
    await fireEvent.click((await screen.findAllByRole('button', { name: 'Import world' }))[1]);
    expect(store.importer.isOpen).toBe(true);
    store.importer.close();
    mock.worlds = (await new (await import('../mock-api')).MockBackend({ tick: 0 }).listWorlds()).map((w, i) => (i === 0 ? { ...w, edited: Date.now() } : w));
    await store.loadWorlds();
    store.savedEdit = { world: 'Sunny Isle', changes: 4 };
    expect(await screen.findByText('Sunny Isle saved')).toBeInTheDocument();
    expect(screen.getByText('4 changes. The world is back in Worlds.')).toBeInTheDocument();
    expect(screen.getByText('Edited just now')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy to a device…' }));
    expect(store.screen).toBe('transfer');
    store.go('worlds');
    store.savedEdit = { world: 'Nope', changes: 1 };
    await fireEvent.click(await screen.findByRole('button', { name: 'Copy to a device…' }));
    store.go('worlds');
    store.savedEdit = { world: 'Sunny Isle', changes: 1 };
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit again' }));
    expect(store.screen).toBe('worlds');
    expect(store.gameFiles.isOpen).toBe(true);
    store.gameFiles.close();
    store.savedEdit = { world: 'Nope', changes: 1 };
    await fireEvent.click(await screen.findByRole('button', { name: 'Edit again' }));
    mock.fail.import = undefined;
    const real = mock.listWorlds.bind(mock);
    mock.listWorlds = async () => {
      throw { code: 'io', detail: 'x' };
    };
    await store.loadWorlds();
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    mock.listWorlds = real;
  });
});
