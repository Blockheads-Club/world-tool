/**
 * A synthetic world for the mock backend (development without Tauri, unit and e2e tests): hills of grass and dirt
 * over rock with ores, a lake, a cave with a back wall and a small brick house. Our own data, no game files.
 */

import type { Catalog, EditorBlockhead, EditorWorld, IdInfo } from '../types';
import { BLOCK_BYTES, MACRO, encodeOverview } from './data';

export const MOCK_W_MACROS = 16;
export const MOCK_LOW = 12;
export const MOCK_HIGH = 20;

const T = { rock: 1, air: 2, water: 3, dirt: 6, sand: 7, wood: 9, bricks: 11, glass: 24, grass: 27 };

const TILES: [number, string, [number, number, number]][] = [
  [1, 'rock', [110, 110, 115]],
  [2, 'air', [150, 200, 240]],
  [3, 'water', [40, 90, 200]],
  [6, 'dirt', [120, 80, 45]],
  [7, 'sand', [220, 200, 130]],
  [9, 'wood', [150, 100, 50]],
  [11, 'red_bricks', [160, 60, 45]],
  [24, 'glass', [190, 225, 235]],
  [27, 'dirt_grass', [80, 150, 50]],
  [31, 'lava', [240, 90, 20]]
];

/** The mock's catalog (a small part of the real one). */
export function mockCatalog(): Catalog {
  const tiles: IdInfo[] = Array.from({ length: 78 }, (_, id) => ({ id, name: id === 0 ? 'none' : `tile_${id}` }));
  for (const [id, name, color] of TILES) tiles[id] = { id, name, color, ...(id === 1 ? { cell: [0, 1] as [number, number] } : {}) };
  const contents: IdInfo[] = Array.from({ length: 149 }, (_, id) => ({ id, name: id === 0 ? 'nothing' : `contents_${id}` }));
  contents[61] = { id: 61, name: 'copper', color: [210, 120, 60], placeable: true };
  contents[77] = { id: 77, name: 'gold', color: [240, 200, 40], placeable: true, cell: [20, 2] };
  contents[4] = { id: 4, name: 'apple_tree_trunk' };
  const items: IdInfo[] = [
    { id: 4, name: 'STICK', cell: [4, 0] },
    { id: 21, name: 'APPLE' },
    { id: 52, name: 'WOODEN DOOR' },
    { id: 1047, name: 'ROCK', tile: 1 }
  ];
  return { tiles, contents, items };
}

function surface(x: number): number {
  return 520 + Math.round(6 * Math.sin(x / 23) + 3 * Math.sin(x / 7));
}

/** Tile (fg, bg, contents, light) at world position `(x, y)`. */
export function mockTile(x: number, y: number): [number, number, number, number] {
  const s = surface(x);
  if (x >= 300 && x < 340 && y <= 524 && y > s - 4) return y > s - 1 ? [T.water, T.air, 0, 255] : [T.sand, T.sand, 0, 200];
  if (x >= 200 && x < 212 && y > s && y <= s + 8) {
    const wall = x === 200 || x === 211 || y === s + 8;
    return wall ? [T.bricks, T.bricks, 0, 255] : [T.air, T.wood, 0, 220];
  }
  if (y > s) return [T.air, T.air, 0, 255];
  if (y === s) return [T.grass, T.dirt, 0, 255];
  if (y > s - 5) return [T.dirt, T.dirt, 0, 200];
  if (x >= 100 && x < 130 && y > 470 && y < 480) return [T.air, T.rock, 0, 60];
  const ore = (x * 7 + y * 13) % 97 === 0 ? 61 : (x * 11 + y * 5) % 211 === 0 ? 77 : 0;
  return [T.rock, T.rock, ore, Math.max(0, 255 - (s - y) * 8)];
}

/** The view bytes of one block. */
export function mockBlock(mx: number, my: number): Uint8Array {
  const out = new Uint8Array(BLOCK_BYTES);
  for (let i = 0; i < MACRO * MACRO; i++) out.set(mockTile(mx * MACRO + (i % MACRO), my * MACRO + Math.floor(i / MACRO)), i * 4);
  return out;
}

export function mockStored(mx: number, my: number): boolean {
  return mx >= 0 && mx < MOCK_W_MACROS && my >= MOCK_LOW && my < MOCK_HIGH;
}

/** The spawn portal of the mock world. */
export function mockSpawn(): [number, number] {
  return [205, surface(205) + 1];
}

export function mockOverview(scale: number): ArrayBuffer {
  const cat = mockCatalog();
  const w = Math.ceil((MOCK_W_MACROS * MACRO) / scale);
  const h = Math.ceil(1024 / scale);
  const px = new Uint8Array(w * h * 4);
  for (let py = 0; py < h; py++)
    for (let qx = 0; qx < w; qx++) {
      const tx = qx * scale;
      const ty = 1023 - py * scale;
      if (!mockStored(Math.floor(tx / MACRO), Math.floor(ty / MACRO))) continue;
      const [fg] = mockTile(tx, ty);
      const c = cat.tiles[fg].color as [number, number, number];
      px.set([c[0], c[1], c[2], 255], (py * w + qx) * 4);
    }
  return encodeOverview(w, h, px);
}

export function mockBlockheads(): EditorBlockhead[] {
  return [
    { player: '', id: 9, name: 'ADA', x: 205, y: surface(205) + 1, stats: [1, 0.5, 0.72, 0.9], inventory: [[4, 12], [21, 3], [0, 0], [1047, 5]] },
    { player: '', id: 10, name: 'BEN', x: 260, y: surface(260) + 1, stats: null, inventory: null }
  ];
}

export function mockWorld(session: string, libId: string, name: string): EditorWorld {
  const blocks: [number, number][] = [];
  for (let my = MOCK_LOW; my < MOCK_HIGH; my++) for (let mx = 0; mx < MOCK_W_MACROS; mx++) blocks.push([mx, my]);
  return {
    session,
    lib_id: libId,
    world_id: '0123456789abcdef0123456789abcdef',
    name: name.toUpperCase(),
    game_version: '1.7.6',
    width: MOCK_W_MACROS * MACRO,
    height: 1024,
    spawn: mockSpawn(),
    blocks,
    damaged_blocks: 0,
    expert: false,
    created: 1_740_000_000,
    time_of_day: 14,
    server: false,
    blockheads: mockBlockheads()
  };
}
