/** Tile geometry for the tools (same rules as `bhw_editor::geom`). */

import type { Rect } from '../types';

/** The rectangle spanned by two corner tiles (inclusive, any order). */
export function rectOf(a: [number, number], b: [number, number]): Rect {
  const x = Math.min(a[0], b[0]);
  const y = Math.min(a[1], b[1]);
  return { x, y, w: Math.abs(a[0] - b[0]) + 1, h: Math.abs(a[1] - b[1]) + 1 };
}

/** The tiles a brush of `size` covers around `(cx, cy)`, round or square. */
export function brushPoints(cx: number, cy: number, size: number, round: boolean): [number, number][] {
  const n = Math.max(1, Math.min(64, Math.round(size)));
  const lo = -Math.floor((n - 1) / 2);
  const hi = Math.floor(n / 2);
  const r2 = (n / 2) ** 2;
  const off = n % 2 === 0 ? 0.5 : 0;
  const out: [number, number][] = [];
  for (let dy = lo; dy <= hi; dy++)
    for (let dx = lo; dx <= hi; dx++) {
      if (round && n > 2 && (dx - off) ** 2 + (dy - off) ** 2 > r2) continue;
      out.push([cx + dx, cy + dy]);
    }
  return out;
}

/** The tiles on a straight line from `a` to `b` (Bresenham), both ends included. */
export function linePoints(a: [number, number], b: [number, number]): [number, number][] {
  let [x, y] = a;
  const [x1, y1] = b;
  const dx = Math.abs(x1 - x);
  const dy = -Math.abs(y1 - y);
  const sx = x < x1 ? 1 : -1;
  const sy = y < y1 ? 1 : -1;
  let err = dx + dy;
  const out: [number, number][] = [[x, y]];
  while (x !== x1 || y !== y1) {
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y += sy;
    }
    out.push([x, y]);
  }
  return out;
}
