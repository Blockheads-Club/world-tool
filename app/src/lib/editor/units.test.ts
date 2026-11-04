import { describe, expect, it, vi } from 'vitest';
import { images } from './flow.svelte';
import type { FakeCtx } from '../../test-setup';
import { setLang } from '../i18n.svelte';
import type { Catalog } from '../types';
import { clampCam, tileAt, toScreen, toWorld, viewTiles, visibleBlocks, wrapX, zoomAt, zoomPercent, type Camera } from './camera';
import { BLOCK_BYTES, encodeOverview, encodeRegion, key, parseOverview, parseRegion, pretty } from './data';
import { mockBlock, mockCatalog, mockOverview } from './mock-world';
import { MockSession } from './mock-session';
import { contentsName, itemName, tileName } from './names';
import { ALL_LAYERS, BlockCache, blockRes, colorOf, drawBlock, layerKey, type Ctx } from './paint';
import { EditorFlow } from './flow.svelte';
import { MockBackend } from '../mock-api';
import type { AppStore } from '../store.svelte';

const ctx = () => document.createElement('canvas').getContext('2d') as unknown as Ctx & FakeCtx;

describe('editor data', () => {
  it('parses and builds region messages', () => {
    const data = new Uint8Array(BLOCK_BYTES).fill(7);
    const buf = encodeRegion([
      { mx: 300, my: 16, state: 1, data },
      { mx: 301, my: 16, state: 0 },
      { mx: 302, my: 16, state: 2 }
    ]);
    const m = parseRegion(buf);
    expect(m.get(key(300, 16))?.data?.[5]).toBe(7);
    expect(m.get(key(301, 16))).toEqual({ state: 0 });
    expect(m.get(key(302, 16))).toEqual({ state: 2 });
    expect(parseRegion(new ArrayBuffer(3)).size).toBe(0);
  });

  it('parses overview messages', () => {
    const o = parseOverview(encodeOverview(2, 1, new Uint8Array(8).fill(9)));
    expect(o).toMatchObject({ width: 2, height: 1 });
    expect(o?.pixels[7]).toBe(9);
    expect(parseOverview(new ArrayBuffer(4))).toBeNull();
    expect(parseOverview(encodeOverview(2, 2, new Uint8Array(4)))).toBeNull();
    expect(pretty('DIRT_GRASS')).toBe('Dirt grass');
  });

  it('mock world', () => {
    const ms = new MockSession('m');
    expect(parseRegion(ms.region([15, 11, 17, 13], ['15,12'])).size).toBe(2);
    expect(parseRegion(ms.region([0, 31, 1, 40], [])).size).toBe(1);
    expect(mockBlock(3, 16).length).toBe(BLOCK_BYTES);
    expect(parseOverview(mockOverview(8))?.width).toBe(64);
    expect(ms.info(5, 5).stored).toBe(false);
    expect(ms.info(5, 450).fg).toBe(1);
  });
});

describe('images', () => {
  it('decodes PNG bytes with createImageBitmap', async () => {
    const bmp = {};
    vi.stubGlobal('createImageBitmap', async (b: Blob) => (b.type === 'image/png' ? bmp : null));
    expect(await images.decode(new ArrayBuffer(4))).toBe(bmp);
    vi.unstubAllGlobals();
  });
});

describe('camera', () => {
  const s = { w: 200, h: 100 };
  const c: Camera = { x: 50, y: 500, zoom: 10 };

  it('maps between screen and world', () => {
    expect(toWorld(c, s, 100, 50)).toEqual({ x: 50, y: 500 });
    expect(toScreen(c, s, 50, 499)).toEqual({ x: 100, y: 50 });
    expect(tileAt(c, s, 105, 55)).toEqual({ x: 50, y: 499 });
    expect(viewTiles(c, s)).toEqual({ w: 20, h: 10 });
    expect(zoomPercent(32)).toBe(200);
  });

  it('finds visible blocks and keeps the camera in the world', () => {
    expect(visibleBlocks(c, s, 4096, 1024)).toEqual([0, 14, 3, 17]);
    expect(visibleBlocks({ x: 5000, y: 2000, zoom: 1 }, s, 4096, 1024, 0)).toEqual([153, 32, 160, 32]);
    expect(visibleBlocks({ x: 50, y: 500, zoom: 1 }, { w: 9000, h: 100 }, 4096, 1024, 0)[2]).toBe(-140 + 128);
    expect(wrapX(-1, 10)).toBe(9);
    expect(clampCam({ x: -5, y: 5000, zoom: 4 }, 100, 1024)).toEqual({ x: 95, y: 1024, zoom: 4 });
  });

  it('zooms around a point', () => {
    const z = zoomAt({ x: 50, y: 500, zoom: 16 }, s, 100, 50, 1);
    expect(z).toEqual({ x: 50, y: 500, zoom: 32 });
    expect(zoomAt({ x: 0, y: 0, zoom: 64 }, s, 0, 0, 1).zoom).toBe(64);
    expect(zoomAt({ x: 0, y: 0, zoom: 1 }, s, 0, 0, -1).zoom).toBe(1);
    expect(zoomAt({ x: 0, y: 0, zoom: 5 }, s, 0, 0, 1).zoom).toBe(32);
  });
});

describe('painting', () => {
  const cat: Catalog = mockCatalog();
  const one = (fg: number, bg: number, ct: number, light: number) => {
    const d = new Uint8Array(BLOCK_BYTES);
    for (let i = 0; i < 1024; i++) d.set([fg, bg, ct, light], i * 4);
    return d;
  };
  const atlas = document.createElement('canvas');

  it('draws with plain colours', () => {
    const c = ctx();
    drawBlock(c, one(1, 1, 61, 255), 4, cat, null, ALL_LAYERS);
    expect(c.calls.filter((x) => x.startsWith('fillRect')).length).toBe(2048);
    expect(c.calls.some((x) => x.startsWith('drawImage'))).toBe(false);
  });

  it('draws with the atlas: tiles, contents cells, back walls, light, unknown ids', () => {
    let c = ctx();
    drawBlock(c, one(1, 1, 77, 100), 16, cat, atlas, { ...ALL_LAYERS, light: true });
    expect(c.calls.filter((x) => x.startsWith('drawImage')).length).toBe(2048);
    expect(c.calls.filter((x) => x.startsWith('fillRect')).length).toBe(1024);
    c = ctx();
    drawBlock(c, one(2, 9, 0, 255), 16, cat, atlas, ALL_LAYERS);
    expect(c.calls.filter((x) => x.startsWith('fillRect')).length).toBe(2048); // wall colour + shade
    c = ctx();
    drawBlock(c, one(2, 2, 0, 255), 16, cat, atlas, ALL_LAYERS);
    expect(c.calls.length).toBe(1); // only clearRect: sky
    c = ctx();
    drawBlock(c, one(3, 1, 0, 255), 16, cat, atlas, { ...ALL_LAYERS, liquids: false, walls: false });
    expect(c.calls.length).toBe(1);
    c = ctx();
    drawBlock(c, one(50, 0, 4, 255), 16, cat, atlas, ALL_LAYERS);
    expect(c.fillStyle).toBe('rgb(255,0,255)');
    c = ctx();
    drawBlock(c, one(2, 0, 0, 255), 16, cat, atlas, ALL_LAYERS);
    expect(c.calls.length).toBe(1);
  });

  it('chooses resolutions and keys', () => {
    expect(blockRes(1, true)).toBe(4);
    expect(blockRes(64, true)).toBe(16);
    expect(blockRes(64, false)).toBe(4);
    expect(blockRes(2, false)).toBe(2);
    expect(layerKey(ALL_LAYERS)).toBe('1110');
    expect(colorOf(undefined)).toBeNull();
  });

  it('names ids', () => {
    setLang('en');
    expect(tileName(cat, 27)).toBe('Dirt grass');
    expect(tileName(cat, 0)).toBe('Unknown (0)');
    expect(tileName(null, 5)).toBe('Unknown (5)');
    expect(contentsName(cat, 0)).toBe('nothing');
    expect(contentsName(cat, 61)).toBe('Copper');
    expect(contentsName(null, 61)).toBe('Unknown (61)');
    expect(itemName(cat, 21)).toBe('Apple');
    expect(itemName(cat, 999)).toBe('Unknown (999)');
  });
});

describe('block cache', () => {
  it('empties itself when full', () => {
    const c = new BlockCache<number>(2);
    c.set('a', 1);
    c.set('b', 2);
    expect(c.size).toBe(2);
    c.set('c', 3);
    expect(c.size).toBe(1);
    expect(c.get('a')).toBeUndefined();
    expect(c.get('c')).toBe(3);
  });
});

describe('editor flow loading', () => {
  it('loads the visible blocks, stops when closed, and ignores answers for a closed world', async () => {
    const api = new MockBackend({ tick: 0 });
    const failed: unknown[] = [];
    const app = { api, fail: (e: unknown) => failed.push(e) } as unknown as AppStore;
    const f = new EditorFlow(app);
    await f.ensure(); // nothing open
    const world = (await api.listWorlds())[0];
    await f.open(world);
    expect(f.blocks.size).toBeGreaterThan(0);
    const n = f.blocks.size;
    await f.ensure(); // all there already
    expect(f.blocks.size).toBe(n);
    // an answer arriving after the world was closed is dropped
    let release: () => void = () => {};
    const slow = api.editorRegion.bind(api);
    api.editorRegion = (s, r) => new Promise((res) => (release = () => res(slow(s, r))));
    f.panTiles(500, 0);
    const pending = f.ensure();
    f.close();
    release();
    await pending;
    expect(f.blocks.size).toBe(0);
    expect(failed).toEqual([]);
  });
});
