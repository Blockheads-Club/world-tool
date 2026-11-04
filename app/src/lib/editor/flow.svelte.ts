import type {
  AppError,
  Brush,
  Catalog,
  EditRequest,
  EditorBlockhead,
  EditorObject,
  EditorState,
  EditorTile,
  EditorWorld,
  OpKind,
  Outcome,
  Rect,
  SchematicInfo,
  SearchKind,
  TileMatch,
  World
} from '../types';
import { toAppError } from '../errors';
import { t } from '../i18n.svelte';
import type { AppStore } from '../store.svelte';
import { toast } from '../ui/toast.svelte';
import { clampCam, tileAt, visibleBlocks, wrapX, zoomAt, type Camera, type Size } from './camera';
import { key, parseOverview, parseRegion, type BlockData, type Overview } from './data';
import { brushPoints, linePoints, oreBrush, rectOf } from './geom';
import { ALL_LAYERS, type Layers } from './paint';

/** Most macro blocks requested at once (the backend accepts 256). */
export const REQUEST_BLOCKS = 128;
/** Overview scale (tiles per minimap pixel). */
export const OVERVIEW_SCALE = 4;
/** Most tiles a replace preview marks on the map. */
export const PREVIEW_LIMIT = 2000;

export type Panel = 'tile' | 'history' | 'players' | 'world' | 'search';
export type Tool = 'inspect' | 'pencil' | 'brush' | 'remove' | 'select' | 'fill' | 'replace' | 'paste';
export const TOOLS: { tool: Tool; key: string; icon: 'cursor' | 'pencil' | 'brush' | 'eraser' | 'select' | 'fill' | 'swap' | 'paste' }[] = [
  { tool: 'inspect', key: 'i', icon: 'cursor' },
  { tool: 'pencil', key: 'p', icon: 'pencil' },
  { tool: 'brush', key: 'b', icon: 'brush' },
  { tool: 'remove', key: 'e', icon: 'eraser' },
  { tool: 'select', key: 's', icon: 'select' },
  { tool: 'fill', key: 'f', icon: 'fill' },
  { tool: 'replace', key: 'r', icon: 'swap' },
  { tool: 'paste', key: 'v', icon: 'paste' }
];

/** Turns PNG bytes into something `drawImage` can use (replaced in tests: jsdom cannot decode images). */
export const images = {
  decode: async (bytes: ArrayBuffer): Promise<CanvasImageSource> => createImageBitmap(new Blob([bytes], { type: 'image/png' }))
};

const EMPTY_STATE: EditorState = { name: null, time_of_day: null, history: [], cursor: 0, unsaved: 0, clipboard: null };

/**
 * The world editor screen state: the open session, the camera, the tile data loaded so far, the user's atlas (or
 * plain colours), layers, the current tool and its options, selection, paste preview, history and panels.
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
  /** Tile under the pointer (wrapped column). */
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
  /** Per block: bumped when its data was replaced (cached drawings are dropped). */
  blockVersion = new Map<string, number>();

  // editing
  tool = $state<Tool>('inspect');
  block = $state<Brush>({ layer: 'front', tile: 1, contents: 0 });
  brushSize = $state(3);
  brushRound = $state(true);
  onlySolid = $state(false);
  selection = $state<Rect | null>(null);
  /** Tiles of the stroke being drawn (preview). */
  stroke = $state.raw<[number, number][]>([]);
  /** Rectangle being dragged (select / fill). */
  dragRect = $state<Rect | null>(null);
  paste = $state<{ x: number; y: number; rotate: number; flip: boolean; skipAir: boolean } | null>(null);
  ed = $state<EditorState>(EMPTY_STATE);
  busy = $state(false);
  replaceFrom = $state<TileMatch>({ layer: 'front', tile: 1, contents: null });
  replaceWhole = $state(false);
  /** Tiles a replace would change (map preview), and how many in all. */
  preview = $state.raw<[number, number][] | null>(null);
  previewCount = $state<number | null>(null);
  /** Changed tiles shown on the map (compare view). */
  compare = $state.raw<[number, number][] | null>(null);
  schematics = $state<SchematicInfo[]>([]);
  searchKind = $state<SearchKind | 'blocks'>('chests');
  searchText = $state('');
  results = $state<{ x: number; y: number; label: string; object: EditorObject | null }[] | null>(null);
  resultsTotal = $state(0);
  fullscreen = $state(false);
  panelsHidden = $state(false);
  saveAsk = $state(false);
  /** Where to go after the save question. */
  leaveTo = $state<import('../store.svelte').Screen>('worlds');
  saving = $state(false);
  itemPick = $state<{ title: string; apply: (item: number, count: number) => void; item: number; count: number } | null>(null);
  teleport = $state<number | null>(null);
  removeAsk = $state<number | null>(null);

  private pending = new Set<string>();
  private loading: Promise<void> | null = null;

  constructor(private app: AppStore) {}

  private get api() {
    return this.app.api;
  }

  private get w(): EditorWorld {
    return this.world as EditorWorld;
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
      this.ed = { ...EMPTY_STATE, name: world.name, time_of_day: world.time_of_day };
      const b0 = world.blockheads[0];
      const home = world.spawn ?? (b0 ? [b0.x, b0.y] : centre(world));
      this.cam = { x: home[0] + 0.5, y: home[1] + 0.5, zoom: 16 };
      this.status = 'ready';
      this.overview = parseOverview(await this.api.editorOverview(world.session, OVERVIEW_SCALE));
      void this.loadSchematics();
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
    this.blockVersion = new Map();
    this.pending.clear();
    this.selection = null;
    this.paste = null;
    this.preview = null;
    this.previewCount = null;
    this.compare = null;
    this.results = null;
    this.tool = 'inspect';
    this.fullscreen = false;
    this.saveAsk = false;
    this.leaveTo = 'worlds';
    this.ed = EMPTY_STATE;
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
      const mw = Math.ceil(w.width / 32);
      const [x0, y0, x1, y1] = visibleBlocks(this.cam, this.size, w.width, w.height);
      const missing = new Map<number, Set<number>>();
      for (let my = y0; my < y1; my++)
        for (let mx = x0; mx < x1; mx++) {
          const wx = wrapX(mx, mw);
          if (!this.blocks.has(key(wx, my)) && !this.pending.has(key(wx, my))) {
            if (!missing.has(my)) missing.set(my, new Set());
            (missing.get(my) as Set<number>).add(wx);
          }
        }
      if (missing.size === 0) return;
      // one row at a time: the run of consecutive columns starting at the row's first missing one
      const [my, cols] = [...missing][0];
      const sorted = [...cols].sort((a, b) => a - b);
      let rx1 = sorted[0] + 1;
      while (cols.has(rx1) && rx1 - sorted[0] < REQUEST_BLOCKS) rx1++;
      const rx0 = sorted[0];
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

  /** Forgets blocks whose data changed and loads them again. */
  private async reload(blocks: [number, number][]) {
    for (const [mx, my] of blocks) {
      const k = key(mx, my);
      this.blocks.delete(k);
      this.blockVersion.set(k, (this.blockVersion.get(k) ?? 0) + 1);
    }
    this.version++;
    await this.ensure();
  }

  private moved(c: Camera) {
    this.cam = clampCam(c, this.w.width, this.w.height);
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

  /** Centres the view on tile `(x, y)` and, optionally, shows it in the inspector. */
  goTo(x: number, y: number, inspect = true) {
    this.moved({ ...this.cam, x: x + 0.5, y: y + 0.5 });
    if (inspect) void this.inspect(x, y);
  }

  goSpawn() {
    const s = this.w.spawn;
    if (s) this.goTo(s[0], s[1], false);
  }

  resize(w: number, h: number) {
    if (w <= 0 || h <= 0) return;
    this.size = { w, h };
    void this.ensure();
  }

  /** The tile under a screen point (column wrapped into the world). */
  at(sx: number, sy: number) {
    const p = tileAt(this.cam, this.size, sx, sy);
    return { x: wrapX(p.x, this.w.width), y: p.y };
  }

  /** Shows a tile in the inspector (and switches to the Tile panel unless `stay`). */
  async inspect(x: number, y: number, stay = false) {
    if (y < 0 || y >= this.w.height) return;
    if (!stay) this.panel = 'tile';
    try {
      this.tile = await this.api.editorTile(this.w.session, wrapX(x, this.w.width), y);
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

  // ------------------------------------------------------------ tools

  setTool(tool: Tool) {
    if (tool === 'paste' && !this.ed.clipboard) {
      toast(t('edv.nothingCopied'));
      return;
    }
    this.tool = tool;
    this.paste = tool === 'paste' ? { x: Math.floor(this.cam.x), y: Math.floor(this.cam.y), rotate: 0, flip: false, skipAir: true } : null;
    this.stroke = [];
    this.dragRect = null;
    if (tool !== 'replace') this.clearPreview();
    else this.panel = 'tile';
  }

  /** Picks the block under the pointer (right click / Alt). */
  async pickBlock(x: number, y: number) {
    const b = this.blocks.get(key(Math.floor(x / 32), Math.floor(y / 32)));
    if (!b?.data) return;
    const i = ((y % 32) * 32 + (x % 32)) * 4;
    const [fg, bg, ct] = [b.data[i], b.data[i + 1], b.data[i + 2]];
    const info = this.catalog?.contents[ct];
    if (info?.placeable) this.block = oreBrush(info);
    else this.block = fg === 2 && bg !== 2 ? { layer: 'back', tile: bg, contents: 0 } : { layer: 'front', tile: fg, contents: 0 };
  }

  /** Tiles a stroke covers from `a` to `b` with the current tool. */
  strokePoints(a: { x: number; y: number }, b: { x: number; y: number }): [number, number][] {
    const line = linePoints([a.x, a.y], [b.x, b.y]);
    if (this.tool === 'pencil') return line;
    const out = new Map<string, [number, number]>();
    for (const [x, y] of line) for (const p of brushPoints(x, y, this.brushSize, this.brushRound)) out.set(key(...p), p);
    return [...out.values()];
  }

  /** Adds tiles to the stroke being drawn. */
  extendStroke(points: [number, number][]) {
    const seen = new Set(this.stroke.map((p) => key(...p)));
    this.stroke = [...this.stroke, ...points.filter((p) => !seen.has(key(...p)))];
  }

  /** Sends the drawn stroke as one step. */
  async commitStroke() {
    const points = this.stroke.map(([x, y]) => [wrapX(x, this.w.width), y] as [number, number]).filter(([, y]) => y >= 0 && y < this.w.height);
    this.stroke = [];
    if (points.length === 0) return;
    const kind = this.tool as OpKind;
    const brush: Brush = kind === 'remove' ? { layer: 'front', tile: 2, contents: 0 } : this.block;
    await this.edit({ op: 'paint', kind, points, brush, only_solid: this.onlySolid });
  }

  /** A rectangle was dragged with the select or fill tool. */
  async finishRect(rect: Rect) {
    this.dragRect = null;
    const r = { ...rect, x: wrapX(rect.x, this.w.width) };
    if (this.tool === 'fill') await this.edit({ op: 'fill', rect: r, brush: this.block });
    else this.selection = r;
  }

  rectFrom(a: { x: number; y: number }, b: { x: number; y: number }): Rect {
    return rectOf([a.x, a.y], [b.x, b.y]);
  }

  /** Runs one edit; the changed blocks, blockheads and settings are refreshed. */
  async edit(req: EditRequest): Promise<Outcome | null> {
    const w = this.w;
    this.busy = true;
    try {
      const r = await this.api.editorEdit(w.session, req);
      this.ed = r.state;
      this.notice(r.outcome);
      if (['move_blockhead', 'set_stats', 'set_stack', 'remove_blockhead'].includes(req.op)) await this.refreshBlockheads();
      if (req.op === 'set_container_stack' && this.tile) await this.inspect(this.tile.x, this.tile.y);
      if (this.compare) await this.showCompare(true);
      await this.reload(r.blocks);
      return r.outcome;
    } catch (e) {
      // a refused edit is not the phone's "allow this computer" error that shares its code
      if (toAppError(e).code === 'not_allowed') toast(t(req.op === 'move_blockhead' ? 'edv.moveRefused' : 'edv.editRefused'), undefined, 8000);
      else this.app.fail(e);
      return null;
    } finally {
      this.busy = false;
    }
  }

  private notice(o: Outcome) {
    const parts: string[] = [];
    if (o.protected) parts.push(t('edv.n.protected', { n: o.protected }));
    if (o.missing) parts.push(t('edv.n.missing', { n: o.missing }));
    if (o.removed) parts.push(t('edv.n.removed', { n: o.removed }));
    if (o.created) parts.push(t('edv.n.created', { n: o.created }));
    if (parts.length) toast(parts.join(' '), undefined, 8000);
  }

  private async refreshBlockheads() {
    this.blockheads = await this.api.editorBlockheads(this.w.session);
    if (this.picked !== null && this.picked >= this.blockheads.length) this.picked = null;
  }

  /** Undo (`-1`), redo (`+1`) or go to history position `to`. */
  async history(to: number) {
    const w = this.w;
    if (to < 0 || to > this.ed.history.length || to === this.ed.cursor) return;
    try {
      const r = await this.api.editorHistory(w.session, to);
      this.ed = r.state;
      await this.refreshBlockheads();
      if (this.compare) await this.showCompare(true);
      await this.reload(r.blocks);
    } catch (e) {
      this.app.fail(e);
    }
  }

  undo() {
    return this.history(this.ed.cursor - 1);
  }

  redo() {
    return this.history(this.ed.cursor + 1);
  }

  // ------------------------------------------------------------ selection, clipboard, paste

  async copy(cut = false) {
    const r = this.selection;
    if (!r) return;
    await this.edit({ op: cut ? 'cut' : 'copy', rect: r });
  }

  async fillSelection() {
    if (this.selection) await this.edit({ op: 'fill', rect: this.selection, brush: this.block });
  }

  async clearSelection() {
    if (this.selection) await this.edit({ op: 'clear', rect: this.selection });
  }

  /** The size of the paste preview (rotated). */
  pasteSize(): [number, number] {
    const [w, h] = this.ed.clipboard as [number, number];
    return (this.paste as { rotate: number }).rotate % 2 ? [h, w] : [w, h];
  }

  movePaste(x: number, y: number) {
    if (this.paste) this.paste = { ...this.paste, x, y };
  }

  rotatePaste() {
    if (this.paste) this.paste = { ...this.paste, rotate: (this.paste.rotate + 1) % 4 };
  }

  flipPaste() {
    if (this.paste) this.paste = { ...this.paste, flip: !this.paste.flip };
  }

  async placePaste() {
    const p = this.paste;
    if (!p) return;
    await this.edit({ op: 'paste', x: wrapX(p.x, this.w.width), y: p.y, skip_air: p.skipAir, rotate: p.rotate, flip: p.flip });
  }

  // ------------------------------------------------------------ replace

  async previewReplace() {
    const area = this.replaceWhole ? null : this.selection;
    try {
      const [found, n] = await this.api.editorFind(this.w.session, this.replaceFrom, area, PREVIEW_LIMIT);
      this.preview = found;
      this.previewCount = n;
    } catch (e) {
      this.app.fail(e);
    }
  }

  clearPreview() {
    this.preview = null;
    this.previewCount = null;
  }

  async applyReplace() {
    const area = this.replaceWhole ? null : this.selection;
    this.clearPreview();
    await this.edit({ op: 'replace', from: this.replaceFrom, to: this.block, area });
  }

  // ------------------------------------------------------------ compare, search, schematics

  /** Shows (or with `refresh`, updates) the changed tiles on the map; toggles off otherwise. */
  async showCompare(refresh = false) {
    if (this.compare && !refresh) {
      this.compare = null;
      return;
    }
    try {
      this.compare = await this.api.editorChanges(this.w.session);
    } catch (e) {
      this.app.fail(e);
    }
  }

  async search() {
    const w = this.w;
    try {
      if (this.searchKind === 'blocks') {
        const [found, n] = await this.api.editorFind(w.session, { layer: this.block.layer, tile: this.block.tile, contents: this.block.layer === 'front' && this.block.contents ? this.block.contents : null }, null, 200);
        this.results = found.map(([x, y]) => ({ x, y, label: '', object: null }));
        this.resultsTotal = n;
      } else {
        const objs = await this.api.editorSearch(w.session, this.searchKind, this.searchText);
        this.results = objs.map((o) => ({ x: o.x, y: o.y, label: o.text ?? '', object: o }));
        this.resultsTotal = objs.length;
      }
    } catch (e) {
      this.app.fail(e);
    }
  }

  async loadSchematics() {
    try {
      this.schematics = await this.api.editorSchematics();
    } catch (e) {
      this.app.fail(e);
    }
  }

  async saveSchematic(name: string, fromClipboard = false) {
    try {
      await this.api.editorSaveSchematic(this.w.session, fromClipboard ? null : this.selection, name);
      toast(t('edv.sch.saved', { name }));
      await this.loadSchematics();
    } catch (e) {
      this.app.fail(e);
    }
  }

  async useSchematic(file: string) {
    try {
      this.ed = await this.api.editorUseSchematic(this.w.session, file);
      this.setTool('paste');
    } catch (e) {
      this.app.fail(e);
    }
  }

  async importSchematic() {
    const path = await this.api.pickSchematic(false, null);
    if (!path) return;
    try {
      await this.api.editorImportSchematic(path);
      await this.loadSchematics();
    } catch (e) {
      this.app.fail(e);
    }
  }

  async exportSchematic(s: SchematicInfo) {
    const dest = await this.api.pickSchematic(true, s.file);
    if (!dest) return;
    try {
      await this.api.editorExportSchematic(s.file, dest);
      toast(t('edv.sch.exported', { name: s.name }));
    } catch (e) {
      this.app.fail(e);
    }
  }

  async deleteSchematic(s: SchematicInfo) {
    try {
      await this.api.editorDeleteSchematic(s.file);
      await this.loadSchematics();
    } catch (e) {
      this.app.fail(e);
    }
  }

  // ------------------------------------------------------------ saving and leaving

  /** Back to Worlds (asks to save when there are unsaved changes). */
  leave() {
    this.app.go('worlds');
  }

  /** Saves into Worlds (or as a new world) and goes back to Worlds. */
  async save(asNew: boolean) {
    const w = this.w;
    this.saving = true;
    try {
      const saved = await this.api.editorSave(w.session, asNew);
      const changes = this.ed.unsaved;
      this.saveAsk = false;
      this.app.go(this.leaveTo, true);
      this.app.savedEdit = { world: saved.name, changes };
    } catch (e) {
      this.app.fail(e);
    } finally {
      this.saving = false;
    }
  }

  /** Leaves without saving. */
  discard() {
    this.saveAsk = false;
    this.app.go(this.leaveTo, true);
  }
}

function centre(w: EditorWorld): [number, number] {
  const b = w.blocks[Math.floor(w.blocks.length / 2)] ?? [0, 16];
  return [b[0] * 32 + 16, b[1] * 32 + 16];
}
