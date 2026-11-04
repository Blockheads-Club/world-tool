/** Drawing tiles: one macro block into a 2D context, with the user's atlas or with plain colours. */

import type { Catalog, IdInfo } from '../types';
import { AIR, LAVA, MACRO, VIEW_BYTES, WATER } from './data';

/** The parts of a 2D context the renderer uses (so tests can record the calls). */
export interface Ctx {
  fillStyle: string | CanvasGradient | CanvasPattern;
  globalAlpha: number;
  imageSmoothingEnabled: boolean;
  fillRect(x: number, y: number, w: number, h: number): void;
  clearRect(x: number, y: number, w: number, h: number): void;
  drawImage(img: CanvasImageSource, sx: number, sy: number, sw: number, sh: number, dx: number, dy: number, dw: number, dh: number): void;
}

export interface Layers {
  blocks: boolean;
  walls: boolean;
  liquids: boolean;
  light: boolean;
  objects: boolean;
}

export const ALL_LAYERS: Layers = { blocks: true, walls: true, liquids: true, light: false, objects: true };

/** Atlas cell size in the HD files. */
export const CELL = 64;

/** Colour for tiles with an id nobody knows. */
export const UNKNOWN = 'rgb(255,0,255)';

export function rgb(c: [number, number, number]): string {
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

/** The plain colour of an id, `null` if it has none. */
export function colorOf(info: IdInfo | undefined): string | null {
  return info?.color ? rgb(info.color) : null;
}

/** A short key for a layer set (cache keys). */
export function layerKey(l: Layers): string {
  return [l.blocks, l.walls, l.liquids, l.light].map((b) => (b ? 1 : 0)).join('');
}

function cell(ctx: Ctx, atlas: CanvasImageSource | null, info: IdInfo | undefined, x: number, y: number, r: number): void {
  if (atlas && info?.cell) {
    ctx.drawImage(atlas, info.cell[0] * CELL, info.cell[1] * CELL, CELL, CELL, x, y, r, r);
  } else {
    ctx.fillStyle = colorOf(info) ?? UNKNOWN;
    ctx.fillRect(x, y, r, r);
  }
}

/**
 * Draws one block view at `r` pixels per tile into `ctx` (origin = the block's top-left corner). Air shows the
 * back wall darkened (or stays transparent); contents are drawn over the tile (atlas cell, else a small coloured
 * square); the light layer darkens tiles by their light level.
 */
export function drawBlock(ctx: Ctx, data: Uint8Array, r: number, cat: Catalog, atlas: CanvasImageSource | null, layers: Layers): void {
  ctx.clearRect(0, 0, MACRO * r, MACRO * r);
  ctx.imageSmoothingEnabled = false;
  const textured = atlas !== null && r >= 4;
  const img = textured ? atlas : null;
  for (let i = 0; i < MACRO * MACRO; i++) {
    const o = i * VIEW_BYTES;
    const fg = data[o];
    const bg = data[o + 1];
    const ct = data[o + 2];
    const light = data[o + 3];
    const x = (i % MACRO) * r;
    const y = (MACRO - 1 - Math.floor(i / MACRO)) * r;
    const liquid = fg === WATER || fg === LAVA;
    const front = fg !== AIR && layers.blocks && (!liquid || layers.liquids);
    if (!front) {
      if (!layers.walls || bg === AIR || bg === 0) continue;
      cell(ctx, img, cat.tiles[bg], x, y, r);
      ctx.fillStyle = 'rgb(0,0,0)';
      ctx.globalAlpha = 0.45;
      ctx.fillRect(x, y, r, r);
      ctx.globalAlpha = 1;
    } else {
      cell(ctx, img, cat.tiles[fg], x, y, r);
      const c = ct ? cat.contents[ct] : undefined;
      if (c?.cell && img) cell(ctx, img, c, x, y, r);
      else if (c?.color) {
        const s = Math.max(1, Math.floor(r / 3));
        ctx.fillStyle = rgb(c.color);
        ctx.fillRect(x + s, y + s, Math.max(1, r - 2 * s), Math.max(1, r - 2 * s));
      }
    }
    if (layers.light && light < 255) {
      ctx.fillStyle = 'rgb(0,0,0)';
      ctx.globalAlpha = (1 - light / 255) * 0.8;
      ctx.fillRect(x, y, r, r);
      ctx.globalAlpha = 1;
    }
  }
}

/** Resolution (pixels per tile) a block is cached at for a zoom: textures need at least 4 px; never above 16. */
export function blockRes(zoom: number, textured: boolean): number {
  if (!textured) return Math.min(zoom, 4);
  return Math.max(4, Math.min(16, zoom));
}

/** A bounded cache of drawn blocks (emptied when full: blocks are cheap to redraw). */
export class BlockCache<T> {
  private map = new Map<string, T>();
  constructor(readonly max: number) {}
  get(k: string): T | undefined {
    return this.map.get(k);
  }
  set(k: string, v: T) {
    if (this.map.size >= this.max) this.map.clear();
    this.map.set(k, v);
  }
  get size() {
    return this.map.size;
  }
}
