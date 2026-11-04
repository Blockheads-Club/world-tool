import { describe, expect, it } from 'vitest';
import { setLang } from '../i18n.svelte';
import { MockBackend } from '../mock-api';
import type { Catalog } from '../types';
import { brushPoints, linePoints, oreBrush, rectOf } from './geom';
import { part, stepText } from './history';
import { MockSession, mockObjects } from './mock-session';
import { mockCatalog, mockSpawn } from './mock-world';
import { brushName, itemInfo } from './names';

const cat: Catalog = mockCatalog();

describe('geometry', () => {
  it('rectangles, brushes, lines, ores', () => {
    expect(rectOf([5, 9], [3, 7])).toEqual({ x: 3, y: 7, w: 3, h: 3 });
    expect(brushPoints(5, 5, 1, true)).toEqual([[5, 5]]);
    expect(brushPoints(5, 5, 4, true).length).toBe(12);
    expect(brushPoints(5, 5, 3, true).length).toBe(9);
    expect(brushPoints(5, 5, 2, false).length).toBe(4);
    expect(brushPoints(0, 0, 0, false).length).toBe(1);
    expect(linePoints([0, 0], [3, 1])).toEqual([[0, 0], [1, 0], [2, 1], [3, 1]]);
    expect(linePoints([3, 3], [0, 0]).length).toBe(4);
    expect(linePoints([0, 3], [0, 0]).length).toBe(4);
    expect(oreBrush(1).tile).toBe(6);
    expect(oreBrush(2).tile).toBe(6);
    expect(oreBrush(64).tile).toBe(12);
    expect(oreBrush(61)).toEqual({ layer: 'front', tile: 1, contents: 61 });
  });
});

describe('names and history labels', () => {
  it('reads tokens and pictures', () => {
    setLang('en');
    expect(part(cat, undefined)).toBe('');
    expect(part(cat, 'ADA')).toBe('ADA');
    expect(part(cat, 't:1')).toBe('Rock');
    expect(part(cat, 't:1:61')).toBe('Copper');
    expect(part(cat, 'b:6')).toBe('background Dirt');
    expect(part(cat, 'i:21')).toBe('Apple');
    expect(stepText(cat, { kind: 'replace', tiles: 3, a: 't:1', b: 't:6' })).toBe('Replace Rock → Dirt · 3 tiles');
    expect(brushName(cat, { layer: 'front', tile: 1, contents: 61 })).toBe('Copper');
    expect(brushName(cat, { layer: 'front', tile: 6, contents: 0 })).toBe('Dirt');
    expect(itemInfo(cat, 4)).toMatchObject({ items: true });
    expect(itemInfo(cat, 999)).toEqual({ info: undefined, items: true });
    const withBlock: Catalog = { ...cat, items: [...cat.items, { id: 1047, name: 'ROCK', tile: 1 }] };
    expect(itemInfo(withBlock, 1047)).toEqual({ info: cat.tiles[1], items: false });
    expect(itemInfo(cat, 21)).toMatchObject({ items: true });
    setLang('tr');
    expect(part(cat, 't:1')).toBe('Kaya');
    setLang('en');
  });
});

describe('mock session', () => {
  it('edits like the real one', () => {
    const s = new MockSession('test');
    const [sx, sy] = mockSpawn();
    const front = { layer: 'front' as const, tile: 6 };
    // paint, back layer, only solid, out of range, ungenerated, objects
    expect(s.edit({ op: 'paint', kind: 'pencil', points: [[sx + 2, sy + 30], [sx + 2, sy + 30]], brush: front }).outcome.changed).toBe(1);
    expect(s.edit({ op: 'paint', kind: 'brush', points: [[sx + 3, sy + 30]], brush: { layer: 'back', tile: 9 } }).outcome.changed).toBe(1);
    expect(s.edit({ op: 'paint', kind: 'brush', points: [[sx, sy + 40]], brush: front, only_solid: true }).outcome.changed).toBe(0);
    expect(s.edit({ op: 'paint', kind: 'pencil', points: [[-1, 0], [5, 900]], brush: front }).outcome.missing).toBe(2);
    s.generate = true;
    const g = s.edit({ op: 'paint', kind: 'pencil', points: [[5, 900], [6, 900]], brush: front });
    expect([g.outcome.created, g.outcome.changed]).toEqual([1, 2]);
    const at = s.state().cursor;
    s.goTo(at - 1);
    expect(s.tile(5, 900)).toBeNull();
    s.goTo(at);
    expect(s.tile(6, 900)![0]).toBe(6);
    expect(s.edit({ op: 'paint', kind: 'pencil', points: [[-1, 0]], brush: front }).outcome.missing).toBe(1);
    s.objectsEditable = false;
    expect(s.edit({ op: 'paint', kind: 'pencil', points: [[sx, sy]], brush: front }).outcome.protected).toBe(1);
    s.objectsEditable = true;
    expect(s.edit({ op: 'paint', kind: 'pencil', points: [[sx, sy]], brush: front }).outcome.removed).toBe(1);
    // fill, clear, replace (area, whole world, back and contents matches)
    s.edit({ op: 'fill', rect: { x: sx - 1, y: sy + 3, w: 2, h: 2 }, brush: front });
    s.edit({ op: 'clear', rect: { x: sx - 1, y: sy + 3, w: 1, h: 1 } });
    expect(s.find({ layer: 'back', tile: 9 }, null, 5)[1]).toBeGreaterThan(1);
    expect(s.find({ layer: 'front', tile: 1, contents: 61 }, null, 1)[0].length).toBe(1);
    expect(s.find({ layer: 'front', tile: 1, contents: null }, { x: 0, y: 400, w: 2, h: 2 }, 1)[1]).toBe(4);
    s.edit({ op: 'replace', from: { layer: 'front', tile: 6 }, to: { layer: 'front', tile: 7 }, area: { x: sx - 1, y: sy + 3, w: 2, h: 2 } });
    // copy, cut, paste (rotate, flip, skip air), errors
    expect(() => new MockSession('x').edit({ op: 'paste', x: 0, y: 0 })).toThrow();
    s.edit({ op: 'copy', rect: { x: sx - 1, y: sy - 1, w: 3, h: 2 } });
    s.edit({ op: 'cut', rect: { x: sx - 1, y: sy - 1, w: 1, h: 1 } });
    expect(s.state().clipboard).toEqual([1, 1]);
    s.edit({ op: 'copy', rect: { x: sx - 1, y: sy + 30, w: 3, h: 2 } });
    expect(s.edit({ op: 'paste', x: 10, y: 450, skip_air: true }).outcome.changed).toBe(0);
    s.edit({ op: 'copy', rect: { x: 10, y: 440, w: 3, h: 2 } });
    s.clipboard!.cells[0] = null;
    expect(s.edit({ op: 'paste', x: 20, y: 500, rotate: 1, flip: true }).outcome.changed).toBe(5);
    // players, containers, settings
    s.edit({ op: 'move_blockhead', player: '', id: 9, x: 1, y: 2 });
    s.edit({ op: 'set_stats', player: '', id: 9, stats: [1, 1, 1, 1] });
    s.edit({ op: 'set_stack', player: '', id: 9, slot: 0, item: 21, count: 0 });
    s.edit({ op: 'set_stack', player: '', id: 9, slot: 1, item: 21, count: 2 });
    expect(() => s.edit({ op: 'set_stack', player: '', id: 10, slot: 0, item: 21, count: 1 })).toThrow();
    expect(() => s.edit({ op: 'move_blockhead', player: '', id: 77, x: 1, y: 1 })).toThrow();
    new MockSession('c').edit({ op: 'set_container_stack', key: mockObjects()[0].key, index: 0, slot: 1, item: 4, count: 0 });
    expect(() => s.edit({ op: 'set_container_stack', key: 'nope', index: 0, slot: 0, item: 4, count: 1 })).toThrow();
    expect(() => s.edit({ op: 'set_name', name: ' ' })).toThrow();
    s.edit({ op: 'set_name', name: 'x' });
    s.edit({ op: 'set_time_of_day', hour: 3 });
    s.edit({ op: 'remove_blockhead', player: '', id: 10 });
    // history: back to the start and forward again, saved state, changes
    const n = s.state().history.length;
    s.markSaved();
    expect(s.goTo(0).blocks.length).toBeGreaterThan(0);
    expect(s.state().unsaved).toBe(n);
    s.edit({ op: 'set_time_of_day', hour: 4 });
    expect(s.state().unsaved).toBe(1);
    expect(() => s.goTo(99)).toThrow();
    s.goTo(0);
    s.goTo(1);
    expect(s.changes().length).toBe(0);
    s.edit({ op: 'paint', kind: 'pencil', points: [[sx + 9, sy + 30]], brush: front });
    expect(s.changes()).toEqual([[sx + 9, sy + 30]]);
    // search
    expect(s.search('chests', '').length).toBe(1);
    expect(s.search('signs', 'ADA').length).toBe(1);
    expect(s.search('objects', 'chest').length).toBe(1);
    expect(s.search('objects', 'zzz').length).toBe(0);
    expect(s.info(5, 5).stored).toBe(false);
  });

  it('mock backend schematics and errors', async () => {
    const m = new MockBackend({ tick: 0 });
    const w = await m.editorOpen((await m.listWorlds())[0].id);
    await expect(m.editorSaveSchematic(w.session, null, 'x')).rejects.toMatchObject({ code: 'not_allowed' });
    expect((await m.editorSaveSchematic(w.session, { x: 0, y: 0, w: 2, h: 2 }, ' ')).file).toBe('Schematic.bhwschem');
    expect((await m.editorSaveSchematic(w.session, { x: 0, y: 0, w: 2, h: 2 }, '')).file).toBe('Schematic (2).bhwschem');
    await expect(m.editorUseSchematic(w.session, 'nope')).rejects.toMatchObject({ code: 'not_found' });
    expect(await m.pickSchematic(false, null)).toBe('/home/ada/Tower.bhwschem');
    await expect(m.editorExportSchematic('nope', '/x')).rejects.toMatchObject({ code: 'not_found' });
  });
});
