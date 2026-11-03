import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Import world, drop files, files opened by the OS', () => {
  it('browses files and a folder, resolves conflicts and imports', async () => {
    const { mock } = await setup();
    await fireEvent.click(screen.getByRole('button', { name: 'Worlds' }));
    await screen.findByRole('list');
    await fireEvent.click(screen.getByRole('button', { name: 'Import world' }));
    const dlg = await screen.findByRole('dialog', { name: 'Import worlds' });
    expect(within(dlg).getByRole('button', { name: 'Import 0 worlds' })).toBeDisabled();
    await fireEvent.click(within(dlg).getByRole('button', { name: 'browse files' }));
    await within(dlg).findByText('Frozen Peaks.bhworld');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'browse files' }));
    expect(within(dlg).getAllByText('Frozen Peaks.bhworld')).toHaveLength(1);
    await fireEvent.click(within(dlg).getByRole('button', { name: 'choose a folder' }));
    await within(dlg).findByText('skyblock_world');
    mock.picks.files = ['/d/notes.txt', '/s/Deep Mine'];
    await fireEvent.click(within(dlg).getByRole('button', { name: 'choose a folder' }));
    await within(dlg).findByText('notes.txt');
    mock.picks.files = [];
    await fireEvent.click(within(dlg).getByRole('button', { name: 'choose a folder' }));
    expect(within(dlg).getByText('Same world ID as “Sunny Isle” in Worlds')).toBeInTheDocument();
    expect(within(dlg).getByText('Same name as “Deep Mine” in Worlds')).toBeInTheDocument();
    expect(within(dlg).getByText(/Blockheads Server folder ·/)).toBeInTheDocument();
    expect(within(dlg).getByText('Not a world file')).toBeInTheDocument();
    expect(within(dlg).getByText('Skipped')).toBeInTheDocument();
    expect(within(dlg).getByText(/Keep both adds the copy/)).toBeInTheDocument();
    await fireEvent.click(within(within(dlg).getByRole('radiogroup', { name: 'Sunny Isle.bhworld: what to do' })).getByRole('radio', { name: 'Skip' }));
    await fireEvent.click(within(within(dlg).getByRole('radiogroup', { name: 'Deep Mine: what to do' })).getByRole('radio', { name: 'Replace' }));
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Import 3 worlds' }));
    expect(await screen.findByText('3 worlds imported')).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('shows the drop overlay and imports dropped or OS-opened files', async () => {
    const { mock } = await setup();
    mock.drag({ type: 'over', count: 4 });
    expect(await screen.findByText('Drop to import')).toBeInTheDocument();
    expect(screen.getByText(/· 4 items/)).toBeInTheDocument();
    mock.drag({ type: 'leave' });
    await waitFor(() => expect(screen.queryByText('Drop to import')).toBeNull());
    mock.drag({ type: 'drop', paths: ['/d/Frozen Peaks.bhworld'] });
    expect(store.screen).toBe('worlds');
    const dlg = await screen.findByRole('dialog', { name: 'Import worlds' });
    await within(dlg).findByText('Frozen Peaks.bhworld');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    mock.openFiles([]);
    expect(store.importer.isOpen).toBe(false);
    mock.openFiles(['/d/Gift.bhworld']);
    await screen.findByText('Gift.bhworld');
    mock.fail.import = { code: 'io' };
    await store.importer.add(['/x.bhworld']);
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    mock.importWorlds = async () => {
      throw { code: 'disk_full', needed: 1, free: 0 };
    };
    await fireEvent.click(screen.getByRole('button', { name: 'Import 1 world' }));
    expect(await screen.findByRole('dialog', { name: 'Not enough disk space' })).toBeInTheDocument();
  });
});
