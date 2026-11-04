import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';
import { toasts } from '../ui/toast.svelte';
import { images } from './flow.svelte';

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

const ed = () => store.editor;
const steps = () => ed().ed.history.length;
/** Screen point of tile (x, y) with the default camera (spawn at the centre, 16 px per tile). */
const px = (x: number, y: number) => ({ clientX: 400 + (x - ed().cam.x) * 16 + 8, clientY: 300 - (y - ed().cam.y) * 16 - 8 });

async function drag(map: HTMLElement, a: [number, number], b: [number, number], extra: Record<string, unknown> = {}) {
  await fireEvent.pointerDown(map, { ...px(...a), button: 0, pointerId: 1, ...extra });
  await fireEvent.pointerMove(map, { ...px(...b), pointerId: 1 });
  await fireEvent.pointerUp(map, { ...px(...b), pointerId: 1 });
}

describe('World editor (editing)', () => {
  it('paints with pencil, brush and eraser; undo, redo, history, compare; picks blocks', async () => {
    const { mock } = await setup();
    const map = await openEditor();
    const [sx, sy] = ed().world!.spawn!;
    // pencil (P): one tile
    await fireEvent.keyDown(window, { key: 'p' });
    expect(screen.getByRole('button', { name: 'Pencil (P)' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText(/Pencil: click or drag/)).toBeInTheDocument();
    await drag(map, [sx + 3, sy + 3], [sx + 5, sy + 3]);
    await waitFor(() => expect(steps()).toBe(1));
    expect(screen.getByText('1 unsaved change')).toBeInTheDocument();
    // brush (B): size and shape
    await fireEvent.keyDown(window, { key: 'b' });
    await fireEvent.input(screen.getByRole('slider'), { target: { value: '5' } });
    await fireEvent.click(screen.getByRole('radio', { name: 'Square' }));
    expect(ed().brushRound).toBe(false);
    await fireEvent.click(screen.getByRole('radio', { name: 'Round' }));
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Only replace solid tiles' }));
    await drag(map, [sx - 3, sy - 6], [sx - 1, sy - 6]);
    await waitFor(() => expect(steps()).toBe(2));
    // an open-only stroke with "only solid" changes nothing (no step)
    await drag(map, [sx + 2, sy + 40], [sx + 2, sy + 40]);
    await waitFor(() => expect(ed().busy).toBe(false));
    expect(steps()).toBe(2);
    // eraser (E)
    await fireEvent.keyDown(window, { key: 'e' });
    await drag(map, [sx, sy - 2], [sx, sy - 2]);
    await waitFor(() => expect(steps()).toBe(3));
    // undo / redo: keys, buttons, history panel
    await fireEvent.keyDown(window, { key: 'z', ctrlKey: true });
    await waitFor(() => expect(ed().ed.cursor).toBe(2));
    await fireEvent.keyDown(window, { key: 'y', ctrlKey: true });
    await waitFor(() => expect(ed().ed.cursor).toBe(3));
    await fireEvent.keyDown(window, { key: 'Z', ctrlKey: true, shiftKey: true });
    await fireEvent.click(screen.getByRole('button', { name: 'Undo (Ctrl Z)' }));
    await waitFor(() => expect(ed().ed.cursor).toBe(2));
    await fireEvent.click(screen.getByRole('button', { name: 'Redo (Ctrl Y)' }));
    await waitFor(() => expect(ed().ed.cursor).toBe(3));
    await fireEvent.click(screen.getByRole('radio', { name: 'History' }));
    expect(screen.getByText('Pencil · Rock · 3 tiles')).toBeInTheDocument();
    expect(screen.getByText(/Remove \d+ tiles/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open working copy' }));
    await waitFor(() => expect(ed().ed.cursor).toBe(0));
    expect(screen.getAllByText('undone').length).toBe(3);
    await fireEvent.click(screen.getByRole('button', { name: /Brush · Rock/ }));
    await waitFor(() => expect(ed().ed.cursor).toBe(2));
    await ed().history(2); // already there
    await ed().history(99); // out of range
    await fireEvent.click(screen.getByRole('button', { name: 'Compare unsaved changes' }));
    await waitFor(() => expect(screen.getByText(/changed tiles marked on the map/)).toBeInTheDocument());
    await ed().undo(); // compare refreshes
    await fireEvent.click(screen.getByRole('button', { name: 'Hide changes' }));
    expect(ed().compare).toBeNull();
    // right click picks: a back wall behind air, a tile, an ore
    await fireEvent.pointerDown(map, { ...px(sx + 4, sy + 5), button: 2 });
    expect(ed().block.layer).toBe('back');
    await fireEvent.pointerDown(map, { ...px(sx, sy - 10), button: 0, altKey: true });
    expect(ed().block).toMatchObject({ layer: 'front', tile: 1 });
    await ed().pickBlock(5, 5); // not loaded: nothing
    const ore = (() => {
      for (let y = sy - 20; y < sy - 5; y++) for (let x = sx - 20; x < sx + 20; x++) if ((x * 7 + y * 13) % 97 === 0) return [x, y];
      return [0, 0];
    })();
    await ed().pickBlock(ore[0], ore[1]);
    expect(ed().block.contents).toBe(61);
    await fireEvent.pointerDown(map, { ...px(sx, sy), button: 1 });
    // contextmenu is suppressed on the map
    expect(await fireEvent.contextMenu(map)).toBe(false);
    // an edit that fails shows the error
    mock.fail.edit = { code: 'io' };
    await fireEvent.keyDown(window, { key: 'p' });
    await drag(map, [sx + 1, sy + 1], [sx + 1, sy + 1]);
    await waitFor(() => expect(toasts.items.some((x) => x.message.includes('Something went wrong'))).toBe(true));
    // history errors too
    mock.editors.clear();
    await ed().history(0);
    await ed().showCompare();
  });

  it('selects, copies, cuts, fills, clears; pastes with rotate and flip; schematics', async () => {
    const { mock } = await setup();
    const map = await openEditor();
    const [sx, sy] = ed().world!.spawn!;
    // paste without anything copied
    await fireEvent.keyDown(window, { key: 'v', ctrlKey: true });
    expect(toasts.items.some((x) => x.message.includes('Copy a selection'))).toBe(true);
    await fireEvent.keyDown(window, { key: 's' });
    expect(screen.getByText('Drag on the map to select an area.')).toBeInTheDocument();
    await drag(map, [sx - 2, sy - 3], [sx + 1, sy - 1]);
    expect(ed().selection).toEqual({ x: sx - 2, y: sy - 3, w: 4, h: 3 });
    expect(screen.getByText(/4 × 3 tiles · 12/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    await waitFor(() => expect(ed().ed.clipboard).toEqual([4, 3]));
    await fireEvent.click(screen.getByRole('button', { name: 'Fill' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    await waitFor(() => expect(steps()).toBe(2));
    await fireEvent.keyDown(window, { key: 'x', ctrlKey: true });
    await fireEvent.keyDown(window, { key: 'c', ctrlKey: true });
    await fireEvent.keyDown(window, { key: 'Delete' });
    await fireEvent.click(screen.getByRole('button', { name: 'Cut' }));
    await waitFor(() => expect(ed().busy).toBe(false));
    // save the selection and the clipboard as schematics
    await fireEvent.input(screen.getByRole('textbox', { name: 'Schematic name' }), { target: { value: 'Hut' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.getByText('Hut')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Save the copied area as a schematic' }));
    await waitFor(() => expect(mock.schematics.length).toBe(4));
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Export Hut' }));
    await waitFor(() => expect(toasts.items.some((x) => x.message === 'Hut exported')).toBe(true));
    mock.picks.schematic = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Export Hut' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Import…' }));
    mock.picks.schematic = '/x/Tower.bhwschem';
    await fireEvent.click(screen.getByRole('button', { name: 'Import…' }));
    await waitFor(() => expect(mock.schematics[0].name).toBe('Tower'));
    await waitFor(() => expect(screen.getByText('Tower')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Delete Tower' }));
    await waitFor(() => expect(screen.queryByText('Tower')).toBeNull());
    // deselect, Escape
    await fireEvent.click(screen.getByRole('button', { name: 'Deselect' }));
    expect(ed().selection).toBeNull();
    await drag(map, [sx, sy], [sx + 1, sy + 1]);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(ed().selection).toBeNull();
    await fireEvent.keyDown(window, { key: 'Escape' }); // nothing to cancel
    // use a schematic: paste mode with preview, arrows, rotate, flip, skip air, place
    await fireEvent.click(screen.getByRole('button', { name: /^Small house 11/ }));
    await waitFor(() => expect(ed().tool).toBe('paste'));
    await fireEvent.pointerMove(map, px(sx, sy));
    const p0 = { ...ed().paste! };
    await fireEvent.keyDown(map, { key: 'ArrowRight' });
    await fireEvent.keyDown(map, { key: 'ArrowUp', shiftKey: true });
    expect(ed().paste).toMatchObject({ x: p0.x + 1, y: p0.y + 1 });
    await fireEvent.click(screen.getByRole('button', { name: 'Rotate' }));
    expect(ed().pasteSize()).toEqual([9, 11]);
    await fireEvent.click(screen.getByRole('button', { name: 'Flip' }));
    await fireEvent.click(screen.getByRole('checkbox', { name: /Leave air out/ }));
    await fireEvent.keyDown(map, { key: 'Enter' });
    await waitFor(() => expect(ed().ed.history.at(-1)?.kind).toBe('paste'));
    await fireEvent.pointerDown(map, { ...px(sx, sy), button: 0, pointerId: 1 });
    await fireEvent.pointerUp(map, { ...px(sx, sy), pointerId: 1 });
    await fireEvent.click(screen.getByRole('button', { name: 'Paste · Enter' }));
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(ed().tool).toBe('inspect');
    ed().movePaste(1, 1);
    ed().rotatePaste();
    ed().flipPaste();
    await ed().placePaste();
    await fireEvent.keyDown(window, { key: 'v' });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(ed().tool).toBe('inspect');
    // fill tool: drag a rectangle
    await fireEvent.keyDown(window, { key: 'f' });
    expect(screen.getByText(/the area is filled/)).toBeInTheDocument();
    const before = steps();
    await drag(map, [sx - 1, sy + 2], [sx + 1, sy + 3]);
    await waitFor(() => expect(steps()).toBe(before + 1));
    // nothing selected: the selection actions do nothing
    ed().selection = null;
    await ed().copy();
    await ed().fillSelection();
    await ed().clearSelection();
    // schematic errors
    mock.editors.clear();
    await ed().loadSchematics();
    await ed().saveSchematic('x');
    await ed().useSchematic('Farm plot.bhwschem');
    mock.picks.schematic = '/x/y.bhwschem';
    mock.fail.import = { code: 'world_damaged' };
    await ed().importSchematic();
    await ed().exportSchematic({ file: 'gone', name: 'gone', width: 1, height: 1, source_world: null, created: 0 });
    vi.spyOn(mock, 'editorDeleteSchematic').mockRejectedValueOnce({ code: 'io' });
    await ed().deleteSchematic({ file: 'gone', name: 'gone', width: 1, height: 1, source_world: null, created: 0 });
  });

  it('replaces with a preview, searches, edits players, containers and world settings', async () => {
    const { mock } = await setup();
    const map = await openEditor();
    const [sx, sy] = ed().world!.spawn!;
    // replace in a selection, then in the whole world
    await fireEvent.keyDown(window, { key: 's' });
    await drag(map, [sx - 6, sy - 8], [sx + 6, sy - 6]);
    await fireEvent.keyDown(window, { key: 'r' });
    expect(screen.getByText(/With: Rock/)).toBeInTheDocument();
    await fireEvent.change(screen.getByRole('combobox', { name: 'Replace' }), { target: { value: '1' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    await waitFor(() => expect(screen.getByText(/tiles will change/)).toBeInTheDocument());
    // pick dirt as the replacement from the palette (search)
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Find a block' }), { target: { value: 'dirt' } });
    await fireEvent.click(within(screen.getByRole('listbox', { name: 'Block palette' })).getByRole('option', { name: 'Dirt' }));
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Find a block' }), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(ed().ed.history.at(-1)?.kind).toBe('replace'));
    await fireEvent.click(screen.getByRole('radio', { name: 'Whole world' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    await waitFor(() => expect(ed().previewCount).not.toBeNull());
    await fireEvent.click(screen.getByRole('radio', { name: 'Selection' }));
    ed().selection = null;
    await waitFor(() => expect(screen.getByText(/Select an area first/)).toBeInTheDocument());
    // back wall layer and ore entries in the palette
    await fireEvent.click(screen.getByRole('radio', { name: 'Background wall' }));
    expect(ed().block.layer).toBe('back');
    await fireEvent.click(within(screen.getByRole('listbox', { name: 'Block palette' })).getByRole('option', { name: 'Copper' }));
    expect(ed().block).toMatchObject({ layer: 'front', tile: 1, contents: 61 });
    await fireEvent.click(screen.getByRole('radio', { name: 'Background wall' }));
    await fireEvent.click(within(screen.getByRole('listbox', { name: 'Block palette' })).getByRole('option', { name: 'Sand' }));
    expect(ed().block).toMatchObject({ layer: 'back', tile: 7 });
    await fireEvent.click(screen.getByRole('radio', { name: 'Block' }));
    // search: chests, signs, objects, selected block; click a result
    await fireEvent.click(screen.getByRole('radio', { name: 'Search' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Chests' }));
    await waitFor(() => expect(screen.getByText('1 found')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('button', { name: 'Signs by text' }));
    await waitFor(() => expect(screen.getByText(/Ada’s home/)).toBeInTheDocument());
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Name or text' }), { target: { value: 'zzz' } });
    await fireEvent.submit(screen.getByRole('searchbox', { name: 'Name or text' }).closest('form')!);
    await waitFor(() => expect(screen.getByText('0 found')).toBeInTheDocument());
    await fireEvent.input(screen.getByRole('searchbox', { name: 'Name or text' }), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: 'All objects' }));
    await waitFor(() => expect(screen.getByText('2 found')).toBeInTheDocument());
    expect(screen.getByText('Unknown (99) · “Ada’s home”')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /^Chest X/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Selected block' }));
    await waitFor(() => expect(ed().resultsTotal).toBeGreaterThan(0));
    await fireEvent.click(screen.getAllByRole('button', { name: /X \d+ · Y \d+/ })[0]);
    // tile inspector: edit the chest's contents with the item picker
    await ed().inspect(sx, sy);
    await waitFor(() => expect(screen.getByText('2 of 4 slots used')).toBeInTheDocument());
    await fireEvent.click(screen.getAllByRole('button', { name: 'Empty slot' })[0]);
    let dlg = await screen.findByRole('dialog', { name: /Chest · slot 2/ });
    await fireEvent.input(within(dlg).getByRole('searchbox'), { target: { value: 'apple' } });
    await fireEvent.click(within(dlg).getByRole('option', { name: 'Apple' }));
    await fireEvent.input(within(dlg).getByRole('textbox'), { target: { value: '1000' } });
    expect(within(dlg).getByRole('alert')).toHaveTextContent('0 to 999');
    await fireEvent.input(within(dlg).getByRole('textbox'), { target: { value: '7' } });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(ed().ed.history.at(-1)?.kind).toBe('container'));
    await waitFor(() => expect(screen.getByText('3 of 4 slots used')).toBeInTheDocument());
    // players: needs, inventory, teleport, remove
    await fireEvent.click(screen.getByRole('radio', { name: 'Players' }));
    await fireEvent.click(screen.getByRole('button', { name: /ADA/ }));
    await fireEvent.change(screen.getByRole('spinbutton', { name: 'Health' }), { target: { value: '50' } });
    await waitFor(() => expect(ed().blockheads[0].stats![0]).toBe(0.5));
    await fireEvent.change(screen.getByRole('spinbutton', { name: 'Health' }), { target: { value: 'x' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Stick · 12' }));
    dlg = await screen.findByRole('dialog', { name: /ADA · slot 1/ });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Empty the slot' }));
    await waitFor(() => expect(ed().blockheads[0].inventory![0]).toEqual([0, 0]));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Empty slot' })[0]);
    dlg = await screen.findByRole('dialog');
    await fireEvent.input(within(dlg).getByRole('textbox'), { target: { value: '0' } });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Apply' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Empty slot' })[0]);
    dlg = await screen.findByRole('dialog');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Teleport…' }));
    dlg = await screen.findByRole('dialog', { name: 'Move ADA to' });
    const [ix, iy] = within(dlg).getAllByRole('textbox');
    await fireEvent.input(ix, { target: { value: '100' } });
    await fireEvent.input(iy, { target: { value: '500' } });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Teleport…' }));
    await waitFor(() => expect(ed().blockheads[0]).toMatchObject({ x: 100, y: 500 }));
    await fireEvent.click(screen.getByRole('button', { name: 'Teleport…' }));
    await fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove blockhead' }));
    dlg = await screen.findByRole('alertdialog', { name: 'Remove ADA?' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove blockhead' }));
    dlg = await screen.findByRole('alertdialog', { name: 'Remove ADA?' });
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Remove blockhead' }));
    await waitFor(() => expect(ed().blockheads.length).toBe(1));
    ed().picked = 5;
    await ed().undo();
    expect(ed().picked).toBeNull();
    // world: rename, time, options (settings)
    await fireEvent.click(screen.getByRole('radio', { name: 'World' }));
    const name = screen.getByRole('textbox', { name: 'Name' });
    await fireEvent.input(name, { target: { value: 'Moon Base' } });
    await fireEvent.change(name);
    await waitFor(() => expect(screen.getByText('MOON BASE')).toBeInTheDocument());
    await fireEvent.change(name); // same name: nothing
    await fireEvent.input(name, { target: { value: ' ' } });
    await fireEvent.change(name);
    await fireEvent.change(screen.getByRole('slider'), { target: { value: '6' } });
    await waitFor(() => expect(screen.getByText('Time of day · 06:00')).toBeInTheDocument());
    await fireEvent.click(screen.getByRole('switch', { name: 'Edit areas the game has not generated' }));
    await waitFor(() => expect(mock.settings.editor_generate).toBe(true));
    await fireEvent.click(screen.getByRole('switch', { name: 'Change tiles of objects' }));
    await waitFor(() => expect(mock.settings.editor_objects).toBe(false));
    // with generating on, a pencil stroke in the sky above the stored blocks creates an area
    ed().goTo(sx, 700, false);
    await fireEvent.keyDown(window, { key: 'p' });
    await drag(map, [sx, 700], [sx, 700]);
    await waitFor(() => expect(toasts.items.some((x) => x.message.includes('new area created'))).toBe(true));
    // objects protected now
    ed().goTo(sx, sy, false);
    await ed().edit({ op: 'paint', kind: 'pencil', points: [[sx, sy]], brush: { layer: 'front', tile: 1 } });
    await waitFor(() => expect(toasts.items.some((x) => x.message.includes('belongs to an object'))).toBe(true));
    mock.settings.editor_objects = true;
    await ed().edit({ op: 'paint', kind: 'pencil', points: [[sx + 1, sy], [9999, 0]], brush: { layer: 'front', tile: 1 } });
    await waitFor(() => expect(toasts.items.some((x) => x.message.includes('object removed'))).toBe(true));
    mock.settings.editor_generate = false;
    await ed().edit({ op: 'paint', kind: 'pencil', points: [[sx, 900]], brush: { layer: 'front', tile: 1 } });
    await waitFor(() => expect(toasts.items.some((x) => x.message.includes('not generated'))).toBe(true));
    // search and find errors
    mock.editors.clear();
    await ed().search();
    ed().searchKind = 'chests';
    await ed().search();
    await ed().previewReplace();
  });

  it('saves, asks before leaving, full screen', async () => {
    const { mock } = await setup();
    let map = await openEditor();
    const [sx, sy] = ed().world!.spawn!;
    expect(screen.getByRole('button', { name: 'Save to Worlds' })).toBeDisabled();
    // full screen: F11, Tab, Escape, the Exit button, the toolbar button
    await fireEvent.keyDown(window, { key: 'F11' });
    expect(ed().fullscreen).toBe(true);
    expect(screen.getByText(/Full screen · Esc or F11/)).toBeInTheDocument();
    await fireEvent.keyDown(window, { key: 'Tab' });
    expect(ed().panelsHidden).toBe(true);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(ed().fullscreen).toBe(false);
    await fireEvent.keyDown(window, { key: 'Tab' }); // only in full screen
    await fireEvent.click(screen.getByRole('button', { name: 'Full screen (F11)' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Exit' }));
    expect(ed().fullscreen).toBe(false);
    // keys that do nothing
    await fireEvent.keyDown(window, { key: 'k' });
    await fireEvent.keyDown(window, { key: 'q', ctrlKey: true });
    await fireEvent.keyDown(window, { key: 's', ctrlKey: true });
    expect(ed().saveAsk).toBe(false);
    // make a change; leaving asks: cancel, then other screens ask too
    await fireEvent.keyDown(window, { key: 'p' });
    await fireEvent.pointerDown(map, { ...px(sx + 2, sy + 2), button: 0, pointerId: 1 });
    await fireEvent.pointerUp(map, { ...px(sx + 2, sy + 2), pointerId: 1 });
    await waitFor(() => expect(ed().ed.unsaved).toBe(1));
    await fireEvent.keyDown(window, { key: 's', ctrlKey: true });
    let dlg = await screen.findByRole('alertdialog', { name: 'Save changes to SUNNY ISLE?' });
    expect(dlg).toHaveTextContent('Pencil · Rock · 1 tiles');
    await fireEvent.keyDown(window, { key: 'g' }); // ignored under a dialog
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Back to Worlds' }));
    dlg = await screen.findByRole('alertdialog');
    // a failed save keeps the dialog
    mock.fail.save = { code: 'disk_full' };
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Save to Worlds' }));
    await waitFor(() => expect(store.shown?.error.code).toBe('disk_full'));
    store.shown = null;
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Don’t save' }));
    expect(store.screen).toBe('worlds');
    // save to Worlds from the header: back in Worlds with the notice
    map = await openEditor();
    await fireEvent.keyDown(window, { key: 'p' });
    await drag(map, [sx + 2, sy + 2], [sx + 2, sy + 2]);
    await waitFor(() => expect(ed().ed.unsaved).toBe(1));
    await fireEvent.click(screen.getByRole('button', { name: 'Save to Worlds' }));
    await waitFor(() => expect(store.screen).toBe('worlds'));
    expect(store.savedEdit).toEqual({ world: 'Sunny isle', changes: 1 });
    // save as a new world from the leave question asked by the sidebar
    map = await openEditor();
    await fireEvent.keyDown(window, { key: 'p' });
    await drag(map, [sx + 3, sy + 2], [sx + 3, sy + 2]);
    await waitFor(() => expect(ed().ed.unsaved).toBe(1));
    await fireEvent.click(screen.getAllByRole('button', { name: /^Devices/ })[0]);
    dlg = await screen.findByRole('alertdialog');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Save as new world' }));
    await waitFor(() => expect(store.screen).toBe('devices'));
    expect(mock.worlds[0].name).toBe('Sunny isle (2)');
    // nothing to save: leaving does not ask
    await openEditor('Sunny isle');
    await fireEvent.click(screen.getByRole('button', { name: 'Save as new world' }));
    await waitFor(() => expect(store.screen).toBe('worlds'));
  });
});
