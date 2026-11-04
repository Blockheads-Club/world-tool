import type { AppError, Catalog, EditorBlockhead, EditorTile, EditorWorld, World } from '../types';
import { toAppError } from '../errors';
import type { AppStore } from '../store.svelte';
import { clampCam, tileAt, visibleBlocks, zoomAt, type Camera, type Size } from './camera';
import { key, parseOverview, parseRegion, type BlockData, type Overview } from './data';
import { ALL_LAYERS, type Layers } from './paint';

/** Most macro blocks requested at once (the backend accepts 256). */
export const REQUEST_BLOCKS = 128;
/** Overview scale (tiles per minimap pixel). */
export const OVERVIEW_SCALE = 4;

export type Panel = 'tile' | 'players' | 'world';

/** Turns PNG bytes into something `drawImage` can use (replaced in tests: jsdom cannot decode images). */
export const images = {
  decode: async (bytes: ArrayBuffer): Promise<CanvasImageSource> => createImageBitmap(new Blob([bytes], { type: 'image/png' }))
};

/**
 * The world viewer/editor screen state: the open session, the camera, the tile data loaded so far, the user's
 * atlas (or plain colours), layers and the inspector.
 */
export class EditorFlow {
  status = $state<'idle' | 'opening' | 'ready' | 'error'>('idle');
  error = $state<AppError | null>(null);
  /** The library world being edited. */
  source = $state<World | null>(null);
  world = $state<EditorWorld | null>(null);
  catalog = $state<Catalog | null>(null);
  /** The user's TileMap atlas, `null` = plain colours. */
  atlas = $state.raw<CanvasImageSource | null>(null);
  itemsAtlas = $state.raw<CanvasImageSource | null>(null);
  cam = $state<Camera>({ x: 0, y: 0, zoom: 16 });
  size = $state<Size>({ w: 800, h: 600 });
  layers = $state<Layers>({ ...ALL_LAYERS });
  /** Tile under the pointer. */
  hover = $state<{ x: number; y: number } | null>(null);
  /** Tile shown in the inspector. */
  tile = $state<EditorTile | null>(null);
  panel = $state<Panel>('tile');
  blockheads = $state<EditorBlockhead[]>([]);
  /** Selected blockhead (index into `blockheads`). */
  picked = $state<number | null>(null);
  overview = $state.raw<Overview | null>(null);
  goToOpen = $state(false);
  /** Bumped whenever block data arrives (the map redraws). */
  version = $state(0);
  /** Loaded block data by `"mx,my"`. */
  blocks = new Map<string, BlockData>();
  private pending = new Set<string>();
  private loading: Promise<void> | null = null;

  constructor(private app: AppStore) {}

  private get api() {
    return this.app.api;
  }

  /** Opens a Worlds world: a working copy is loaded, then the map around the spawn. */
  async open(w: World) {
    this.close();
    this.source = w;
    this.status = 'opening';
    try {
      const [world, catalog, tiles, items] = await Promise.all([
        this.api.editorOpen(w.id),
        this.catalog ? Promise.resolve(this.catalog) : this.api.editorCatalog(),
        this.api.editorAtlas('tiles'),
        this.api.editorAtlas('items')
      ]);
      this.catalog = catalog;
      this.atlas = tiles.byteLength ? await images.decode(tiles) : null;
      this.itemsAtlas = items.byteLength ? await images.decode(items) : null;
      this.world = world;
      this.blockheads = world.blockheads;
      const b0 = world.blockheads[0];
      const home = world.spawn ?? (b0 ? [b0.x, b0.y] : centre(world));
      this.cam = { x: home[0] + 0.5, y: home[1] + 0.5, zoom: 16 };
      this.status = 'ready';
      this.overview = parseOverview(await this.api.editorOverview(world.session, OVERVIEW_SCALE));
      await this.ensure();
    } catch (e) {
      this.error = toAppError(e);
      this.status = 'error';
    }
  }

  /** Drops the working copy (unsaved changes are lost). */
  close() {
    if (this.world) void this.api.editorClose(this.world.session);
    this.world = null;
    this.source = null;
    this.status = 'idle';
    this.error = null;
    this.tile = null;
    this.hover = null;
    this.picked = null;
    this.blockheads = [];
    this.overview = null;
    this.blocks = new Map();
    this.pending.clear();
    this.version++;
  }

  /** Loads the visible blocks that are not loaded yet (one request at a time; repeats until all are there). */
  async ensure(): Promise<void> {
    if (this.loading) return this.loading;
    this.loading = this.load().finally(() => (this.loading = null));
    return this.loading;
  }

  private async load() {
    for (;;) {
      const w = this.world;
      if (!w) return;
      const [x0, y0, x1, y1] = visibleBlocks(this.cam, this.size, w.width, w.height);
      const missing: [number, number][] = [];
      for (let my = y0; my < y1; my++)
        for (let mx = x0; mx < x1; mx++) if (!this.blocks.has(key(mx, my)) && !this.pending.has(key(mx, my))) missing.push([mx, my]);
      if (missing.length === 0) return;
      // one row band at a time, at most REQUEST_BLOCKS blocks
      const my = missing[0][1];
      const row = missing.filter((m) => m[1] === my).slice(0, REQUEST_BLOCKS);
      const rx0 = row[0][0];
      const rx1 = row[row.length - 1][0] + 1;
      for (let mx = rx0; mx < rx1; mx++) this.pending.add(key(mx, my));
      try {
        const got = parseRegion(await this.api.editorRegion(w.session, [rx0, my, rx1, my + 1]));
        if (this.world !== w) return;
        for (const [k, b] of got) this.blocks.set(k, b);
        this.version++;
      } catch (e) {
        if (this.world === w) this.app.fail(e);
        return;
      } finally {
        for (let mx = rx0; mx < rx1; mx++) this.pending.delete(key(mx, my));
      }
    }
  }

  private moved(c: Camera) {
    const w = this.world as EditorWorld;
    this.cam = clampCam(c, w.width, w.height);
    void this.ensure();
  }

  /** Moves the view by screen pixels. */
  pan(dx: number, dy: number) {
    this.moved({ ...this.cam, x: this.cam.x - dx / this.cam.zoom, y: this.cam.y + dy / this.cam.zoom });
  }

  /** Moves the view by tiles (keyboard). */
  panTiles(dx: number, dy: number) {
    this.moved({ ...this.cam, x: this.cam.x + dx, y: this.cam.y + dy });
  }

  /** One zoom step at a screen point (default: the centre). */
  zoom(dir: number, sx = this.size.w / 2, sy = this.size.h / 2) {
    this.moved(zoomAt(this.cam, this.size, sx, sy, dir));
  }

  /** Centres the view on tile `(x, y)` and selects it in the inspector. */
  goTo(x: number, y: number, inspect = true) {
    this.moved({ ...this.cam, x: x + 0.5, y: y + 0.5 });
    if (inspect) void this.inspect(x, y);
  }

  goSpawn() {
    const s = (this.world as EditorWorld).spawn;
    if (s) this.goTo(s[0], s[1], false);
  }

  resize(w: number, h: number) {
    if (w <= 0 || h <= 0) return;
    this.size = { w, h };
    void this.ensure();
  }

  /** The tile under a screen point. */
  at(sx: number, sy: number) {
    return tileAt(this.cam, this.size, sx, sy);
  }

  /** Shows a tile in the inspector. */
  async inspect(x: number, y: number) {
    const w = this.world as EditorWorld;
    if (x < 0 || y < 0 || x >= w.width || y >= w.height) return;
    this.panel = 'tile';
    try {
      this.tile = await this.api.editorTile(w.session, x, y);
    } catch (e) {
      this.app.fail(e);
    }
  }

  toggle(layer: keyof Layers) {
    this.layers = { ...this.layers, [layer]: !this.layers[layer] };
  }

  /** Selects a blockhead in the Players panel and shows it. */
  pick(i: number) {
    this.panel = 'players';
    this.picked = i;
    const b = this.blockheads[i];
    this.goTo(b.x, b.y, false);
  }
}

function centre(w: EditorWorld): [number, number] {
  const b = w.blocks[Math.floor(w.blocks.length / 2)] ?? [0, 16];
  return [b[0] * 32 + 16, b[1] * 32 + 16];
}
