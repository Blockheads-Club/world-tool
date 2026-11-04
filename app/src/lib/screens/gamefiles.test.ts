import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

async function edit(name = 'Sunny Isle') {
  await fireEvent.click(screen.getAllByRole('button', { name: 'Worlds' })[0]);
  const card = await screen.findByRole('listitem', { name });
  await fireEvent.click(within(card).getByRole('button', { name: 'Edit' }));
}

describe('Game files (APK) before the editor', () => {
  it('asks for the APK, checks, extracts, opens the editor placeholder', async () => {
    const { mock } = await setup({ tick: 15, before: (m) => (m.game = { state: 'not_set', apk_path: null, apk_name: null, version: null, images: null, size: null }) });
    await edit();
    const dlg = await screen.findByRole('dialog', { name: 'Add your game file to edit worlds' });
    expect(within(dlg).getByText('your own copy')).toBeInTheDocument();
    store.dragging = 1;
    expect(await within(dlg).findByText('Drop the APK file to use it')).toBeInTheDocument();
    expect(screen.queryByText('Drop to import')).toBeNull();
    mock.drag({ type: 'drop', paths: ['/d/TheBlockheads.apk'] });
    expect(await screen.findByRole('dialog', { name: 'Checking your game file' })).toBeInTheDocument();
    expect(await screen.findByText('App: The Blockheads')).toBeInTheDocument();
    expect(await screen.findByRole('dialog', { name: 'Getting your game graphics' }, { timeout: 2000 })).toBeInTheDocument();
    expect(await screen.findByText(/Block textures ·/)).toBeInTheDocument();
    const ready = await screen.findByRole('dialog', { name: 'Ready to edit' }, { timeout: 3000 });
    expect(ready).toHaveTextContent('1084 images');
    await fireEvent.click(within(ready).getByRole('button', { name: 'Open Sunny Isle' }));
    expect(store.screen).toBe('editor');
    expect(await screen.findByRole('application', { name: /World map/ })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Back to Worlds' }));
    expect(store.screen).toBe('worlds');
    // now ready: Edit goes straight to the editor
    await edit('Deep Mine');
    expect(store.editing?.name).toBe('Deep Mine');
    expect(await screen.findByText('DEEP MINE')).toBeInTheDocument();
  });

  it('shows the APK errors: wrong app, unsupported, damaged (with phone), other; cancel and not now', async () => {
    const { mock } = await setup({ before: (m) => (m.game.state = 'not_set') });
    await edit();
    let dlg = await screen.findByRole('dialog', { name: 'Add your game file to edit worlds' });
    mock.picks.apk = null;
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Choose APK…' })[1]);
    mock.picks.apk = '/d/Other.apk';
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Choose APK…' })[0]);
    dlg = await screen.findByRole('alertdialog', { name: 'This isn’t The Blockheads' });
    expect(dlg).toHaveTextContent('Wrong app');
    mock.fail.apk = { code: 'apk_unsupported', version: '1.6.3', name: 'TheBlockheads.apk' };
    mock.picks.apk = '/d/TheBlockheads.apk';
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Choose another APK…' }));
    dlg = await screen.findByRole('alertdialog', { name: 'This game version isn’t supported' });
    expect(dlg).toHaveTextContent('1.6.3');
    expect(dlg).toHaveTextContent('Not supported');
    mock.fail.apk = { code: 'apk_damaged', detail: 'E_APK_INCOMPLETE · assets/textures missing' };
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Choose another APK…' }));
    dlg = await screen.findByRole('alertdialog', { name: 'This APK is damaged' });
    expect(dlg).toHaveTextContent('E_APK_INCOMPLETE');
    expect(dlg).toHaveTextContent('Damaged');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Copy details' }));
    mock.fail.apk = { code: 'io' };
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Get it from Ada’s Pixel 8' }));
    dlg = await screen.findByRole('alertdialog', { name: 'The game file could not be used' });
    expect(dlg).toHaveTextContent('Something went wrong');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Copy details' }));
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Not now' }));
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    // pull from the phone, then cancel while checking
    mock.tick = 30;
    store.gameFiles.open();
    await fireEvent.click(await screen.findByRole('button', { name: 'Get it from Ada’s Pixel 8' }));
    expect(await screen.findByRole('progressbar', { name: 'Copying the game from Ada’s Pixel 8' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(store.gameFiles.isOpen).toBe(false));
    await waitFor(() => expect(store.gameFiles.stage).toBe('ask'), { timeout: 2000 });
  });

  it('re-extract from Settings ends with Done; game file missing; no phone', async () => {
    await setup({ before: (m) => { m.game.state = 'missing'; m.devices = m.devices.filter((d) => d.kind !== 'android'); } });
    store.gameFiles.open();
    const dlg = await screen.findByRole('dialog', { name: 'Game file not found' });
    expect(dlg).toHaveTextContent('/home/ada/Downloads/TheBlockheads.apk');
    expect(within(dlg).queryByRole('button', { name: /Get it from/ })).toBeNull();
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Close' }));
    await store.gameFiles.reextract();
    const ready = await screen.findByRole('dialog', { name: 'Ready to edit' });
    await fireEvent.click(within(ready).getByRole('button', { name: 'Settings' }));
    expect(store.screen).toBe('settings');
    await store.gameFiles.reextract();
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'Ready to edit' })).getByRole('button', { name: 'Done' }));
    expect(store.gameFiles.isOpen).toBe(false);
    store.game = null;
    store.gameFiles.open();
    expect(await screen.findByRole('dialog', { name: 'Add your game file to edit worlds' })).toBeInTheDocument();
    store.gameFiles.stage = 'checking';
    store.gameFiles.progress = null;
    expect(await screen.findByText('It is an Android app file')).toBeInTheDocument();
    store.gameFiles.stage = 'extracting';
    expect(await screen.findByRole('progressbar', { name: 'Extracting graphics' })).toBeInTheDocument();
    store.gameFiles.stage = 'error';
    store.gameFiles.error = { code: 'apk_damaged' };
    expect(await screen.findByRole('alertdialog', { name: 'This APK is damaged' })).toBeInTheDocument();
    store.gameFiles.error = { code: 'apk_unsupported' };
    expect(await screen.findByText('?', { selector: 'dd' })).toBeInTheDocument();
    store.editing = null;
    store.screen = 'editor';
    store.gameFiles.close();
    expect(await screen.findByText('Editing a copy · from Worlds')).toBeInTheDocument();
  });
});
