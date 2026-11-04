/** The map camera: which world tile is at the centre of the view and how many screen pixels a tile takes. */

/** Pixels per tile at each zoom step; 16 is shown as 100 %. */
export const ZOOMS = [1, 2, 4, 8, 16, 32, 64] as const;
export const BASE_ZOOM = 16;

export interface Camera {
  /** World tile coordinates at the view centre (`y` grows upwards). */
  x: number;
  y: number;
  /** Pixels per tile (one of `ZOOMS`). */
  zoom: number;
}

export interface Size {
  w: number;
  h: number;
}

/** Screen pixel → world tile coordinates (fractional). */
export function toWorld(c: Camera, s: Size, sx: number, sy: number): { x: number; y: number } {
  return { x: c.x + (sx - s.w / 2) / c.zoom, y: c.y - (sy - s.h / 2) / c.zoom };
}

/** World position → screen pixel (the top-left corner of tile `(tx, ty)` when given integers). */
export function toScreen(c: Camera, s: Size, tx: number, ty: number): { x: number; y: number } {
  return { x: (tx - c.x) * c.zoom + s.w / 2, y: (c.y - ty - 1) * c.zoom + s.h / 2 };
}

/** The tile under a screen pixel. */
export function tileAt(c: Camera, s: Size, sx: number, sy: number): { x: number; y: number } {
  const w = toWorld(c, s, sx, sy);
  return { x: Math.floor(w.x), y: Math.floor(w.y) };
}

/**
 * Visible macro blocks `[mx0, my0, mx1, my1)` (one block of margin). Rows are clamped to the world; columns are
 * not (the world wraps around: use [`wrapX`] on them), but span at most the world's width.
 */
export function visibleBlocks(c: Camera, s: Size, worldW: number, worldH: number, margin = 1): [number, number, number, number] {
  const a = toWorld(c, s, 0, s.h);
  const b = toWorld(c, s, s.w, 0);
  const mw = Math.ceil(worldW / 32);
  const mh = Math.ceil(worldH / 32);
  const clamp = (v: number, hi: number) => Math.max(0, Math.min(hi, v));
  const x0 = Math.floor(a.x / 32) - margin;
  return [x0, clamp(Math.floor(a.y / 32) - margin, mh), Math.min(Math.floor(b.x / 32) + 1 + margin, x0 + mw), clamp(Math.floor(b.y / 32) + 1 + margin, mh)];
}

/** A column (tile or macro block) wrapped into `0..width`. */
export function wrapX(x: number, width: number): number {
  return ((x % width) + width) % width;
}

/** Keeps the centre inside the world: rows are clamped, columns wrap around. */
export function clampCam(c: Camera, worldW: number, worldH: number): Camera {
  return { zoom: c.zoom, x: wrapX(c.x, worldW), y: Math.max(0, Math.min(worldH, c.y)) };
}

/** One zoom step in (`dir > 0`) or out, keeping the world point under `(sx, sy)` where it is. */
export function zoomAt(c: Camera, s: Size, sx: number, sy: number, dir: number): Camera {
  const i = ZOOMS.indexOf(c.zoom as (typeof ZOOMS)[number]);
  const next = ZOOMS[Math.max(0, Math.min(ZOOMS.length - 1, (i < 0 ? 4 : i) + Math.sign(dir)))];
  const p = toWorld(c, s, sx, sy);
  return { zoom: next, x: p.x - (sx - s.w / 2) / next, y: p.y + (sy - s.h / 2) / next };
}

/** Zoom as a percentage (100 % = 16 px per tile). */
export function zoomPercent(zoom: number): number {
  return Math.round((zoom / BASE_ZOOM) * 100);
}

/** Number of whole tiles the view shows. */
export function viewTiles(c: Camera, s: Size): { w: number; h: number } {
  return { w: Math.ceil(s.w / c.zoom), h: Math.ceil(s.h / c.zoom) };
}
