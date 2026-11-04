/** Tile data from the backend: region messages (per macro block) and the overview image. */

/** Tiles per macro block side. */
export const MACRO = 32;
/** Bytes per tile in a block view: foreground, back wall, contents, light. */
export const VIEW_BYTES = 4;
/** Bytes of one block view. */
export const BLOCK_BYTES = MACRO * MACRO * VIEW_BYTES;
/** Tile type ids the renderer treats specially. */
export const AIR = 2;
export const WATER = 3;
export const LAVA = 31;

/** 0 = not generated, 1 = stored, 2 = damaged record. */
export type BlockState = 0 | 1 | 2;

export interface BlockData {
  state: BlockState;
  /** 1024 × 4 bytes, bottom row first (only when stored). */
  data?: Uint8Array;
}

export const key = (mx: number, my: number) => `${mx},${my}`;

/** Parses a region message (`editor_region`). */
export function parseRegion(buf: ArrayBuffer): Map<string, BlockData> {
  const v = new DataView(buf);
  const out = new Map<string, BlockData>();
  let o = 0;
  while (o + 4 <= buf.byteLength) {
    const mx = v.getUint16(o, true);
    const my = v.getUint8(o + 2);
    const state = v.getUint8(o + 3) as BlockState;
    o += 4;
    if (state === 1) {
      out.set(key(mx, my), { state, data: new Uint8Array(buf.slice(o, o + BLOCK_BYTES)) });
      o += BLOCK_BYTES;
    } else out.set(key(mx, my), { state });
  }
  return out;
}

/** Builds a region message (the mock backend and tests). */
export function encodeRegion(blocks: { mx: number; my: number; state: BlockState; data?: Uint8Array }[]): ArrayBuffer {
  const size = blocks.reduce((n, b) => n + 4 + (b.state === 1 ? BLOCK_BYTES : 0), 0);
  const out = new Uint8Array(size);
  const v = new DataView(out.buffer);
  let o = 0;
  for (const b of blocks) {
    v.setUint16(o, b.mx, true);
    v.setUint8(o + 2, b.my);
    v.setUint8(o + 3, b.state);
    o += 4;
    if (b.state === 1) {
      out.set(b.data as Uint8Array, o);
      o += BLOCK_BYTES;
    }
  }
  return out.buffer;
}

export interface Overview {
  width: number;
  height: number;
  /** RGBA, top row first. */
  pixels: Uint8ClampedArray;
}

/** Parses an overview message (`editor_overview`); `null` when it is malformed. */
export function parseOverview(buf: ArrayBuffer): Overview | null {
  if (buf.byteLength < 8) return null;
  const v = new DataView(buf);
  const width = v.getUint32(0, true);
  const height = v.getUint32(4, true);
  if (buf.byteLength !== 8 + width * height * 4) return null;
  return { width, height, pixels: new Uint8ClampedArray(buf, 8) };
}

/** Builds an overview message (mock backend, tests). */
export function encodeOverview(width: number, height: number, pixels: Uint8Array): ArrayBuffer {
  const out = new Uint8Array(8 + pixels.length);
  const v = new DataView(out.buffer);
  v.setUint32(0, width, true);
  v.setUint32(4, height, true);
  out.set(pixels, 8);
  return out.buffer;
}

/** `dirt_grass` → `Dirt grass`. */
export function pretty(name: string): string {
  const s = name.replace(/_/g, ' ').toLowerCase();
  return s.charAt(0).toUpperCase() + s.slice(1);
}
