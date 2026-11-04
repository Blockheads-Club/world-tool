import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FakeCtx } from '../../test-setup';
import { store } from '../store.svelte';
import { setup } from '../test-utils';
import { toasts } from '../ui/toast.svelte';
import { images } from './flow.svelte';
import { mockWorld } from './mock-world';

const decode = images.decode;
beforeEach(() => {
  images.decode = async () => document.createElement('canvas');
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    cb(0);
    return 1;
  });
});
afterEach(() => {
  cleanup();
  images.decode = decode;
  vi.unstubAllGlobals();
});

async function openEditor(name = 'Sunny Isle') {
  await fireEvent.click(screen.getAllByRole('button', { name: 'Worlds' })[0]);
  const card = await screen.findByRole('listitem', { name });
  await fireEvent.click(within(card).getByRole('button', { name: 'Edit' }));
  const map = await screen.findByRole('application', { name: /World map/ });
  await waitFor(() => expect(store.editor.blocks.size).toBeGreaterThan(0));
  return map;
}

const ctxOf = (el: Element | null) => (el as HTMLCanvasElement & { __ctx?: FakeCtx }).__ctx as FakeCtx;

describe('World editor (viewer)', () => {
  it('opens a world, draws it, inspects tiles, pans, zooms and goes places', async () => {
    const { mock } = await setup();
    const map = await openEditor();
    expect(screen.getByText('SUNNY ISLE')).toBeInTheDocument();
    expect(screen.getByText('Graphics: plain colours (no game files)')).toBeInTheDocument();
    await waitFor(() => expect(ctxOf(map.querySelector('canvas')).calls.some((c) => c.startsWith('drawImage'))).toBe(true));
    // hover shows coordinates and the tile name; leaving clears them
    await fireEvent.pointerMove(map, { clientX: 400, clientY: 300 });
    expect(store.editor.hover).not.toBeNull();
    expect(document.querySelector('.coord')?.textContent).toMatch(/X \d+/);
    await fireEvent.pointerLeave(map);
    expect(store.editor.hover).toBeNull();
    // click inspects
    await fireEvent.pointerDown(map, { clientX: 400, clientY: 300, button: 0, pointerId: 1 });
    await fireEvent.pointerUp(map, { clientX: 400, clientY: 300, pointerId: 1 });
    await waitFor(() => expect(store.editor.tile).not.toBeNull());
    expect(screen.getByRole('heading', { name: 'Tile' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy coordinates' }));
    await waitFor(() => expect(toasts.items.some((t) => t.message === 'Coordinates copied')).toBe(true));
    // right button does nothing; a drag pans and does not inspect
    await fireEvent.pointerDown(map, { clientX: 10, clientY: 10, button: 2 });
    const before = store.editor.cam.x;
    await fireEvent.pointerDown(map, { clientX: 400, clientY: 300, button: 0, pointerId: 1 });
    await fireEvent.pointerMove(map, { clientX: 401, clientY: 300 });
    await fireEvent.pointerMove(map, { clientX: 300, clientY: 300 });
    await fireEvent.pointerUp(map, { clientX: 300, clientY: 300 });
    expect(store.editor.cam.x).toBeGreaterThan(before);
    await fireEvent.pointerUp(map, { clientX: 300, clientY: 300 });
    // wheel and keys zoom and move
    await fireEvent.wheel(map, { deltaY: -100, clientX: 400, clientY: 300 });
    expect(store.editor.cam.zoom).toBe(32);
    await fireEvent.wheel(map, { deltaY: 100, clientX: 400, clientY: 300 });
    await fireEvent.keyDown(map, { key: '-' });
    expect(store.editor.cam.zoom).toBe(8);
    expect(screen.getByText('Zoom 50%')).toBeInTheDocument();
    await fireEvent.keyDown(map, { key: '+' });
    await fireEvent.keyDown(map, { key: '=' });
    expect(store.editor.cam.zoom).toBe(32);
    const x = store.editor.cam.x;
    await fireEvent.keyDown(map, { key: 'ArrowRight' });
    await fireEvent.keyDown(map, { key: 'ArrowLeft', shiftKey: true });
    await fireEvent.keyDown(map, { key: 'ArrowUp' });
    await fireEvent.keyDown(map, { key: 'ArrowDown' });
    expect(store.editor.cam.x).toBe(x - 28);
    await fireEvent.keyDown(map, { key: 'Enter' });
    await fireEvent.keyDown(map, { key: 'q' });
    // zoom buttons
    await fireEvent.click(screen.getByRole('button', { name: 'Zoom out' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }));
    // go to dialog: G, invalid, valid
    await fireEvent.keyDown(window, { key: 'g' });
    let dlg = await screen.findByRole('dialog', { name: 'Go to' });
    const [ix, iy] = within(dlg).getAllByRole('textbox');
    await fireEvent.input(ix, { target: { value: '99999' } });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Go' }));
    expect(within(dlg).getByRole('alert')).toHaveTextContent('X 0–511');
    await fireEvent.input(ix, { target: { value: '100' } });
    await fireEvent.input(iy, { target: { value: '450' } });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Go' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Go to' })).toBeNull());
    expect(store.editor.cam.x).toBe(100.5);
    await waitFor(() => expect(store.editor.tile?.x).toBe(100));
    await fireEvent.click(screen.getByRole('button', { name: 'Go to coordinates (G)' }));
    dlg = await screen.findByRole('dialog', { name: 'Go to' });
    await fireEvent.keyDown(window, { key: 'g' }); // ignored while the dialog is open
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    await fireEvent.keyDown(window, { key: 'G' });
    dlg = await screen.findByRole('dialog', { name: 'Go to' });
    await fireEvent.keyDown(dlg, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Go to' })).toBeNull());
    // Home goes to the spawn; G in a text field is typing
    await fireEvent.keyDown(window, { key: 'Home' });
    expect(store.editor.cam.x).toBe(205.5);
    await fireEvent.click(screen.getByRole('button', { name: 'Go to the spawn portal' }));
    await fireEvent.keyDown(window, { key: 'x' });
    await fireEvent.keyDown(screen.getByRole('radio', { name: 'Tile' }), { key: 'g' });
    expect(screen.queryByRole('dialog', { name: 'Go to' })).toBeNull();
    // minimap click moves the view
    const mm = screen.getByRole('button', { name: /Minimap/ });
    await fireEvent.click(mm, { clientX: 0, clientY: 0 });
    expect(store.editor.cam.x).toBeLessThan(205);
    // layers
    const walls = screen.getByRole('button', { name: 'Background walls' });
    await fireEvent.click(walls);
    expect(walls).toHaveAttribute('aria-pressed', 'false');
    await fireEvent.click(screen.getByRole('button', { name: 'Objects' }));
    // panels
    await fireEvent.click(screen.getByRole('radio', { name: 'Players' }));
    expect(screen.getByText('Players · 2 blockheads')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /ADA/ }));
    expect(screen.getByText('72%')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Stick' })).toBeInTheDocument();
    expect(screen.getByText('Empty slot')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Show on the map' }));
    await fireEvent.click(screen.getByRole('button', { name: /BEN/ }));
    expect(screen.getByText('No inventory is saved for this blockhead.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Objects' }));
    await fireEvent.click(screen.getByRole('radio', { name: 'World' }));
    expect(screen.getByText('Normal')).toBeInTheDocument();
    expect(screen.getByText('14:00')).toBeInTheDocument();
    expect(screen.getByText('512 × 1024 tiles')).toBeInTheDocument();
    // leaving closes the session
    await fireEvent.click(screen.getByRole('button', { name: 'Back to Worlds' }));
    expect(store.screen).toBe('worlds');
    expect(mock.editors.size).toBe(0);
  });

  it('shows a chest and protection in the inspector, objects and the game graphics', async () => {
    const { mock } = await setup({ before: (m) => (m.atlas = new Uint8Array([1, 2, 3]).buffer) });
    await openEditor();
    expect(screen.getByText('Graphics: your game file · 1.7.6')).toBeInTheDocument();
    const s = store.editor.world!.spawn!;
    await store.editor.inspect(s[0], s[1]);
    expect(await screen.findByText('Chest')).toBeInTheDocument();
    expect(screen.getByText(/Editing tools leave it alone/)).toBeInTheDocument();
    expect(screen.getByText('Unknown (99)')).toBeInTheDocument();
    expect(screen.getByText('“Ada’s home”')).toBeInTheDocument();
    // hover names: ungenerated, a back wall behind air, a tile
    store.editor.hover = { x: 205, y: 560 };
    expect(await screen.findByText('Air', { selector: '.nm' })).toBeInTheDocument();
    store.editor.hover = { x: 5, y: 5 };
    expect(await screen.findByText('Not generated yet', { selector: '.nm' })).toBeInTheDocument();
    store.editor.goTo(115, 475, false);
    await waitFor(() => expect(store.editor.blocks.get('3,14')?.state).toBe(1));
    store.editor.hover = { x: 110, y: 475 };
    expect(await screen.findByText('Rock', { selector: '.nm' })).toBeInTheDocument();
    // inventory icons from the items atlas
    await fireEvent.click(screen.getByRole('radio', { name: 'Players' }));
    await fireEvent.click(screen.getByRole('button', { name: /ADA/ }));
    expect(screen.getByRole('img', { name: 'Stick' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: 'Tile' }));
    await store.editor.inspect(-1, 0);
    await store.editor.inspect(3, 3);
    expect(await screen.findByText('Not generated yet')).toBeInTheDocument();
    // errors from the backend become toasts; the editor stays open
    mock.editors.clear();
    await store.editor.inspect(3, 3);
    store.editor.panTiles(2000, 0);
    await waitFor(() => expect(toasts.items.length).toBeGreaterThan(0));
  });

  it('shows open errors, version and damaged notices; reopening closes the old session', async () => {
    const { mock } = await setup();
    mock.fail.editor = { code: 'world_damaged', detail: 'blocks 3_16' };
    await fireEvent.click(screen.getAllByRole('button', { name: 'Worlds' })[0]);
    const card = await screen.findByRole('listitem', { name: 'Sunny Isle' });
    await fireEvent.click(within(card).getByRole('button', { name: 'Edit' }));
    expect(await screen.findByText('This world could not be opened')).toBeInTheDocument();
    expect(screen.getByText(/The world file is damaged\. blocks 3_16/)).toBeInTheDocument();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Back to Worlds' })[1]);
    // a 1.6 world with damaged blocks, no spawn and no blockheads
    const open = mock.editorOpen.bind(mock);
    mock.editorOpen = async (id: string) => ({ ...(await open(id)), game_version: '1.6.3', damaged_blocks: 2, spawn: null, blockheads: [], time_of_day: null, created: null, server: true, expert: true });
    mock.damagedBlocks = ['6,16'];
    await openEditor();
    expect(screen.getByText('Made with The Blockheads 1.6.3')).toBeInTheDocument();
    expect(screen.getByText('2 damaged blocks are not shown')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Go to the spawn portal' })).toBeDisabled();
    // zoomed out: damaged and ungenerated blocks are drawn as plain shades
    for (let i = 0; i < 4; i++) store.editor.zoom(-1);
    await waitFor(() => expect(store.editor.blocks.get('6,16')?.state).toBe(2), { timeout: 8000 });
    store.editor.zoom(1);
    await waitFor(() => expect(ctxOf(document.querySelector('.map canvas')).calls.some((c) => c.startsWith('fillRect'))).toBe(true));
    store.editor.goSpawn();
    await fireEvent.click(screen.getByRole('radio', { name: 'Players' }));
    expect(screen.getByText('No blockheads are saved in this world.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: 'World' }));
    expect(screen.getByText('Blockheads Server world')).toBeInTheDocument();
    expect(screen.getByText('Expert mode')).toBeInTheDocument();
    // open again while open: the first session is closed
    const first = store.editor.world!.session;
    mock.editorOpen = async (id: string) => {
      mock.editors.set('ed99', id);
      const w = mockWorld('ed99', id, 'x');
      return { ...w, spawn: null, blockheads: [], blocks: [], game_version: '1.5.0', damaged_blocks: 0 };
    };
    await store.editor.open(store.editing!);
    expect(mock.editors.has(first)).toBe(false);
    expect(store.editor.cam.x).toBe(16.5);
    expect(store.editor.status).toBe('ready');
    expect(screen.getByText('Made with The Blockheads 1.5.0')).toBeInTheDocument();
    expect(screen.queryByText(/damaged block/)).toBeNull();
    // no spawn: the view starts at the first blockhead
    mock.editorOpen = async (id: string) => {
      mock.editors.set('ed98', id);
      return { ...mockWorld('ed98', id, 'x'), spawn: null, damaged_blocks: 1, game_version: null };
    };
    await store.editor.open(store.editing!);
    expect(store.editor.cam.x).toBe(205.5);
    expect(await screen.findByText('1 damaged block is not shown')).toBeInTheDocument();
    expect(screen.queryByText(/Made with The Blockheads/)).toBeNull();
    store.editor.resize(0, 0);
    store.editor.resize(640, 480);
    expect(store.editor.size).toEqual({ w: 640, h: 480 });
  });

  it('follows the container size when ResizeObserver exists', async () => {
    let cb: (() => void) | null = null;
    const disconnect = vi.fn();
    vi.stubGlobal('ResizeObserver', class {
      constructor(f: () => void) {
        cb = f;
      }
      observe() {}
      disconnect = disconnect;
    });
    await setup();
    await openEditor();
    expect(cb).not.toBeNull();
    (cb as unknown as () => void)();
    await fireEvent.click(screen.getByRole('button', { name: 'Back to Worlds' }));
    expect(disconnect).toHaveBeenCalled();
  });
});
