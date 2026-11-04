/**
 * An editing session of the mock backend: the synthetic world (`mock-world.ts`) plus changed tiles, history,
 * clipboard, blockheads, objects and world settings, behaving like the real session closely enough for the UI.
 */

import type {
  Brush,
  EditRequest,
  EditResult,
  EditorBlockhead,
  EditorObject,
  EditorState,
  EditorTile,
  HistoryLabel,
  HistoryResult,
  Outcome,
  Rect,
  SearchKind,
  TileMatch
} from '../types';
import { AIR, BLOCK_BYTES, LAVA, MACRO, WATER, encodeRegion, type BlockState } from './data';
import { MOCK_HIGH, MOCK_LOW, MOCK_W_MACROS, mockBlockheads, mockSpawn, mockTile } from './mock-world';

type Tile = [number, number, number, number];
type Snapshot = { blockheads: EditorBlockhead[]; name: string; hour: number; objects: EditorObject[] };
interface Step {
  label: HistoryLabel;
  tiles: [string, Tile | undefined, Tile][];
  created: string[];
  before: Snapshot;
  after: Snapshot;
}

const WIDTH = MOCK_W_MACROS * MACRO;
const key = (x: number, y: number) => `${x},${y}`;
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const open = (t: Tile) => t[0] === AIR || t[0] === WATER || t[0] === LAVA;

/** The objects of the mock world: a chest at the spawn, Ada's sign next to it. */
export function mockObjects(): EditorObject[] {
  const [x, y] = mockSpawn();
  return [
    { key: '6_16/46', index: 0, kind: 46, kind_name: 'chest', id: 77, x, y, text: null, slots: [[4, 3], [0, 0], [21, 1], [0, 0]] },
    { key: '6_16/47', index: 0, kind: 99, kind_name: null, id: null, x: x + 1, y, text: 'Ada’s home', slots: null }
  ];
}

export class MockSession {
  tiles = new Map<string, Tile>();
  /** Macro blocks created in the ungenerated area. */
  created = new Set<string>();
  steps: Step[] = [];
  cursor = 0;
  saved = 0;
  clipboard: { w: number; h: number; cells: (Tile | null)[] } | null = null;
  blockheads = mockBlockheads();
  objects = mockObjects();
  name: string;
  hour = 14;
  /** Edit options (from the settings). */
  generate = false;
  objectsEditable = true;
  private touched = new Set<string>();

  constructor(name: string) {
    this.name = name.toUpperCase();
  }

  stored(mx: number, my: number): boolean {
    return (mx >= 0 && mx < MOCK_W_MACROS && my >= MOCK_LOW && my < MOCK_HIGH) || this.created.has(`${mx},${my}`);
  }

  tile(x: number, y: number): Tile | null {
    if (!this.stored(Math.floor(x / MACRO), Math.floor(y / MACRO))) return null;
    return this.tiles.get(key(x, y)) ?? (this.created.has(`${Math.floor(x / MACRO)},${Math.floor(y / MACRO)}`) ? [AIR, AIR, 0, 255] : mockTile(x, y));
  }

  region(rect: [number, number, number, number], damaged: string[]): ArrayBuffer {
    const [x0, y0, x1, y1] = rect;
    const blocks: { mx: number; my: number; state: BlockState; data?: Uint8Array }[] = [];
    for (let my = y0; my < Math.min(y1, 32); my++)
      for (let mx = x0; mx < Math.min(x1, MOCK_W_MACROS); mx++) {
        if (damaged.includes(`${mx},${my}`)) blocks.push({ mx, my, state: 2 });
        else if (this.stored(mx, my)) {
          const data = new Uint8Array(BLOCK_BYTES);
          for (let i = 0; i < MACRO * MACRO; i++) data.set(this.tile(mx * MACRO + (i % MACRO), my * MACRO + Math.floor(i / MACRO)) as Tile, i * 4);
          blocks.push({ mx, my, state: 1, data });
        } else blocks.push({ mx, my, state: 0 });
      }
    return encodeRegion(blocks);
  }

  info(x: number, y: number): EditorTile {
    const t = this.tile(x, y);
    const objects = this.objects.filter((o) => Math.abs(o.x - x) <= 1 && Math.abs(o.y - y) <= 1);
    const chest = this.objects.some((o) => o.id === 77 && o.x === x && o.y === y);
    const [fg, bg, contents, light] = t ?? [0, 0, 0, 0];
    return {
      x, y, stored: t !== null, fg, bg, contents, light, sun_light: light, paint: [0, 0, 0, 0, 0],
      owner: chest ? 77 : 0, protected: chest, objects
    };
  }

  state(): EditorState {
    return {
      name: this.name,
      time_of_day: this.hour,
      history: this.steps.map((s) => s.label),
      cursor: this.cursor,
      unsaved: this.saved < 0 ? Math.max(this.cursor, 1) : Math.abs(this.cursor - this.saved),
      clipboard: this.clipboard ? [this.clipboard.w, this.clipboard.h] : null
    };
  }

  private snapshot(): Snapshot {
    return clone({ blockheads: this.blockheads, name: this.name, hour: this.hour, objects: this.objects });
  }

  private restore(s: Snapshot) {
    const c = clone(s);
    this.blockheads = c.blockheads;
    this.name = c.name;
    this.hour = c.hour;
    this.objects = c.objects;
  }

  private touch(k: string) {
    const [x, y] = k.split(',').map(Number);
    this.touched.add(`${Math.floor(x / MACRO)},${Math.floor(y / MACRO)}`);
  }

  private takeTouched(): [number, number][] {
    const out = [...this.touched].map((k) => k.split(',').map(Number) as [number, number]);
    this.touched.clear();
    return out;
  }

  /** Applies `f` to tiles as one step (objects' tiles are protected unless object editing is on). */
  private editTiles(label: HistoryLabel, points: [number, number][], f: (t: Tile, p: [number, number]) => Tile | null): Outcome {
    const out: Outcome = { changed: 0, protected: 0, missing: 0, created: 0, removed: 0 };
    const before = this.snapshot();
    const changes: Step['tiles'] = [];
    const created: string[] = [];
    const seen = new Set<string>();
    for (const [x, y] of points) {
      if (seen.has(key(x, y))) continue;
      seen.add(key(x, y));
      let t = x >= 0 && x < WIDTH && y >= 0 && y < 1024 ? this.tile(x, y) : null;
      if (!t) {
        const pos = `${Math.floor(x / MACRO)},${Math.floor(y / MACRO)}`;
        if (!this.generate || x < 0 || x >= WIDTH || y < 0 || y >= 1024) {
          out.missing++;
          continue;
        }
        this.created.add(pos);
        created.push(pos);
        out.created++;
        t = this.tile(x, y) as Tile;
      }
      const n = f(t, [x, y]);
      if (!n || n.join() === t.join()) continue;
      const obj = this.objects.findIndex((o) => o.x === x && o.y === y);
      if (obj >= 0) {
        if (!this.objectsEditable) {
          out.protected++;
          continue;
        }
        this.objects.splice(obj, 1);
        out.removed++;
      }
      changes.push([key(x, y), this.tiles.get(key(x, y)), n]);
      this.tiles.set(key(x, y), n);
      this.touch(key(x, y));
    }
    out.changed = changes.length;
    if (changes.length || created.length) this.push({ ...label, tiles: out.changed }, before, changes, created);
    return out;
  }

  private push(label: HistoryLabel, before: Snapshot, tiles: Step['tiles'] = [], created: string[] = []) {
    this.steps.splice(this.cursor);
    if (this.saved > this.cursor) this.saved = -1;
    this.steps.push({ label, tiles, created, before, after: this.snapshot() });
    this.cursor++;
  }

  private rectPoints(r: Rect): [number, number][] {
    const pts: [number, number][] = [];
    for (let y = r.y; y < r.y + r.h; y++) for (let x = r.x; x < r.x + r.w; x++) pts.push([x, y]);
    return pts;
  }

  private apply(b: Brush) {
    return (t: Tile): Tile => (b.layer === 'back' ? [t[0], b.tile, t[2], t[3]] : [b.tile, t[1], b.contents ?? 0, t[3]]);
  }

  find(target: TileMatch, area: Rect | null, limit: number): [[number, number][], number] {
    const pts = area ? this.rectPoints(area) : this.rectPoints({ x: 0, y: MOCK_LOW * MACRO, w: WIDTH, h: (MOCK_HIGH - MOCK_LOW) * MACRO });
    const found: [number, number][] = [];
    let n = 0;
    for (const [x, y] of pts) {
      const t = this.tile(x, y);
      if (t && matches(target, t)) {
        n++;
        if (found.length < limit) found.push([x, y]);
      }
    }
    return [found, n];
  }

  edit(req: EditRequest): EditResult {
    const none: Outcome = { changed: 0, protected: 0, missing: 0, created: 0, removed: 0 };
    let outcome = none;
    const before = this.snapshot();
    const lbl = (kind: HistoryLabel['kind'], a?: string, b?: string): HistoryLabel => ({ kind, tiles: 0, ...(a ? { a } : {}), ...(b ? { b } : {}) });
    const bh = (player: string, id: number) => {
      const b = this.blockheads.find((x) => x.player === player && x.id === id);
      if (!b) throw { code: 'not_allowed', reason: 'no_blockhead', detail: `no blockhead ${id}` };
      return b;
    };
    switch (req.op) {
      case 'paint': {
        const f = this.apply(req.brush);
        outcome = this.editTiles(lbl(req.kind, req.kind === 'remove' ? undefined : `t:${req.brush.tile}`), req.points, (t) => (req.only_solid && open(t) ? null : f(t)));
        break;
      }
      case 'fill':
        outcome = this.editTiles(lbl('fill', `t:${req.brush.tile}`), this.rectPoints(req.rect), this.apply(req.brush));
        break;
      case 'clear':
        outcome = this.editTiles(lbl('clear'), this.rectPoints(req.rect), (t) => [AIR, t[1], 0, t[3]]);
        break;
      case 'replace': {
        const pts = this.find(req.from, req.area, 1e9)[0];
        outcome = this.editTiles(lbl('replace', `t:${req.from.tile}`, `t:${req.to.tile}`), pts, this.apply(req.to));
        break;
      }
      case 'copy':
      case 'cut': {
        const { x, y, w, h } = req.rect;
        this.clipboard = { w, h, cells: this.rectPoints(req.rect).map(([px, py]) => this.tile(px, py)) };
        if (req.op === 'cut') outcome = this.editTiles(lbl('cut'), this.rectPoints({ x, y, w, h }), (t) => [AIR, t[1], 0, t[3]]);
        break;
      }
      case 'paste': {
        const c = this.clipboard;
        if (!c) throw { code: 'not_allowed', reason: 'nothing_copied', detail: 'nothing copied' };
        let cells = c.cells;
        let w = c.w;
        let h = c.h;
        for (let r = 0; r < (req.rotate ?? 0) % 4; r++) {
          const next: (Tile | null)[] = new Array(w * h);
          for (let yy = 0; yy < h; yy++) for (let xx = 0; xx < w; xx++) next[(w - 1 - xx) * h + yy] = cells[yy * w + xx];
          cells = next;
          [w, h] = [h, w];
        }
        if (req.flip) {
          const f = cells.slice();
          for (let yy = 0; yy < h; yy++) for (let xx = 0; xx < w; xx++) f[yy * w + (w - 1 - xx)] = cells[yy * w + xx];
          cells = f;
        }
        const pts: [number, number][] = [];
        const by = new Map<string, Tile>();
        cells.forEach((t, i) => {
          if (!t || (req.skip_air && t[0] === AIR)) return;
          const p: [number, number] = [req.x + (i % w), req.y + Math.floor(i / w)];
          pts.push(p);
          by.set(key(...p), t);
        });
        outcome = this.editTiles(lbl('paste'), pts, (_t, p) => by.get(key(...p)) as Tile);
        break;
      }
      case 'move_blockhead':
        Object.assign(bh(req.player, req.id), { x: req.x, y: req.y });
        this.push(lbl('blockhead', bh(req.player, req.id).name), before);
        break;
      case 'set_stats':
        bh(req.player, req.id).stats = req.stats;
        this.push(lbl('blockhead', bh(req.player, req.id).name), before);
        break;
      case 'set_stack': {
        const b = bh(req.player, req.id);
        if (!b.inventory) throw { code: 'not_allowed', reason: 'no_inventory', detail: 'no inventory' };
        b.inventory[req.slot] = [req.count ? req.item : 0, req.count];
        this.push(lbl('inventory', b.name, `i:${req.item}`), before);
        break;
      }
      case 'remove_blockhead': {
        const b = bh(req.player, req.id);
        this.blockheads = this.blockheads.filter((x) => x !== b);
        this.push(lbl('remove_blockhead', b.name), before);
        break;
      }
      case 'set_container_stack': {
        const o = this.objects.find((x) => x.key === req.key && x.index === req.index);
        if (!o?.slots) throw { code: 'not_allowed', reason: 'no_items', detail: 'this object holds no items' };
        o.slots[req.slot] = [req.count ? req.item : 0, req.count];
        this.push(lbl('container', undefined, `i:${req.item}`), before);
        break;
      }
      case 'set_name':
        if (!req.name.trim()) throw { code: 'not_allowed', reason: 'bad_name', detail: 'a world name needs 1 to 64 characters' };
        this.name = req.name.trim().toUpperCase();
        this.push(lbl('settings'), before);
        break;
      case 'set_time_of_day':
        this.hour = req.hour;
        this.push(lbl('settings'), before);
        break;
    }
    return { outcome, state: this.state(), blocks: this.takeTouched() };
  }

  goTo(to: number): HistoryResult {
    if (to > this.steps.length) throw { code: 'not_allowed', reason: 'no_step', detail: 'no such step' };
    while (this.cursor > to) {
      const s = this.steps[--this.cursor];
      for (const [k, b] of [...s.tiles].reverse()) {
        if (b) this.tiles.set(k, b);
        else this.tiles.delete(k);
        this.touch(k);
      }
      s.created.forEach((p) => this.created.delete(p));
      this.restore(s.before);
    }
    while (this.cursor < to) {
      const s = this.steps[this.cursor++];
      s.created.forEach((p) => this.created.add(p));
      for (const [k, , a] of s.tiles) {
        this.tiles.set(k, a);
        this.touch(k);
      }
      this.restore(s.after);
    }
    return { state: this.state(), blocks: this.takeTouched() };
  }

  changes(): [number, number][] {
    const set = new Set<string>();
    for (const s of this.steps.slice(0, this.cursor)) for (const [k] of s.tiles) set.add(k);
    return [...set].map((k) => k.split(',').map(Number) as [number, number]);
  }

  search(kind: SearchKind, text: string): EditorObject[] {
    const q = text.trim().toLowerCase();
    return this.objects
      .filter((o) => (kind === 'chests' ? o.slots !== null : kind === 'signs' ? o.text !== null : true))
      .filter((o) => !q || (o.kind_name ?? '').replace(/_/g, ' ').includes(q) || (o.text ?? '').toLowerCase().includes(q));
  }

  markSaved() {
    this.saved = this.cursor;
  }
}

function matches(m: TileMatch, t: Tile): boolean {
  if (m.layer === 'back') return t[1] === m.tile;
  return t[0] === m.tile && (m.contents === undefined || m.contents === null || m.contents === t[2]);
}
