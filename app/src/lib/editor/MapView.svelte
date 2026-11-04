<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import type { EditorWorld, Rect } from '../types';
  import { tileAt, toScreen, visibleBlocks, wrapX } from './camera';
  import { MACRO, key } from './data';
  import { BlockCache, blockRes, drawBlock, layerKey, type Ctx } from './paint';

  /**
   * The map: blocks drawn into cached per-block canvases, composited at the camera's zoom; the world wraps around
   * horizontally. Overlays: selection, stroke, dragged rectangle, paste preview, replace preview, changed tiles,
   * spawn and blockheads, the inspected and hovered tile. Pointer and keys drive the current tool.
   */
  const ed = $derived(store.editor);
  let box: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  /** Drawn blocks; at most 600 before the cache is emptied. */
  const cache = new BlockCache<HTMLCanvasElement>(600);
  let frame = 0;

  $effect(() => {
    const el = box as HTMLDivElement;
    const measure = () => ed.resize(el.clientWidth, el.clientHeight);
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });

  $effect(() => {
    // redraw when any of these change
    void [ed.version, ed.cam, ed.layers, ed.atlas, ed.size, ed.hover, ed.tile, ed.picked, ed.blockheads];
    void [ed.selection, ed.stroke, ed.dragRect, ed.paste, ed.preview, ed.compare, ed.ed.clipboard];
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });

  function blockImage(k: string, data: Uint8Array, res: number): HTMLCanvasElement {
    const ck = `${k},${ed.blockVersion.get(k) ?? 0},${res},${layerKey(ed.layers)},${ed.atlas ? 1 : 0}`;
    const hit = cache.get(ck);
    if (hit) return hit;
    const c = document.createElement('canvas');
    c.width = c.height = MACRO * res;
    drawBlock(c.getContext('2d') as unknown as Ctx, data, res, ed.catalog!, ed.atlas, ed.layers);
    cache.set(ck, c);
    return c;
  }

  function draw() {
    const w = ed.world as EditorWorld;
    const cv = canvas as HTMLCanvasElement;
    const ctx = cv.getContext('2d') as CanvasRenderingContext2D;
    const { w: vw, h: vh } = ed.size;
    const dpr = window.devicePixelRatio;
    if (cv.width !== Math.round(vw * dpr) || cv.height !== Math.round(vh * dpr)) {
      cv.width = Math.round(vw * dpr);
      cv.height = Math.round(vh * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, vw, vh);
    ctx.imageSmoothingEnabled = false;
    const cam = ed.cam;
    const res = blockRes(cam.zoom, ed.atlas !== null);
    const mw = Math.ceil(w.width / MACRO);
    const [x0, y0, x1, y1] = visibleBlocks(cam, ed.size, w.width, w.height, 0);
    const side = MACRO * cam.zoom;
    for (let my = y0; my < y1; my++)
      for (let mx = x0; mx < x1; mx++) {
        const p = toScreen(cam, ed.size, mx * MACRO, my * MACRO + MACRO - 1);
        const k = key(wrapX(mx, mw), my);
        const b = ed.blocks.get(k);
        if (b?.state === 1) {
          const img = blockImage(k, b.data as Uint8Array, res);
          ctx.drawImage(img, 0, 0, img.width, img.height, p.x, p.y, side, side);
        } else if (b) {
          // not generated (dim) or damaged (red)
          ctx.fillStyle = b.state === 2 ? 'rgba(180,35,24,.45)' : 'rgba(31,26,23,.35)';
          ctx.fillRect(p.x, p.y, side, side);
        }
      }
    /** Screen position of world tile `(x, y)`, using the copy of the column nearest the view. */
    const at = (x: number, y: number) => {
      const near = x + Math.round((cam.x - x) / w.width) * w.width;
      return toScreen(cam, ed.size, near, y);
    };
    const rect = (r: Rect, stroke: string, fill: string | null, width = 2) => {
      const p = at(r.x, r.y + r.h - 1);
      if (fill) {
        ctx.fillStyle = fill;
        ctx.fillRect(p.x, p.y, r.w * cam.zoom, r.h * cam.zoom);
      }
      ctx.strokeStyle = stroke;
      ctx.lineWidth = width;
      ctx.strokeRect(p.x, p.y, r.w * cam.zoom, r.h * cam.zoom);
    };
    const tiles = (list: [number, number][], fill: string) => {
      ctx.fillStyle = fill;
      for (const [x, y] of list) {
        const p = at(x, y);
        ctx.fillRect(p.x, p.y, cam.zoom, cam.zoom);
      }
    };
    const mark = (x: number, y: number, color: string, width = 2) => rect({ x, y, w: 1, h: 1 }, color, null, width);
    if (ed.compare) tiles(ed.compare, 'rgba(255,122,69,.55)');
    if (ed.preview) tiles(ed.preview, 'rgba(255,210,122,.6)');
    if (ed.stroke.length) tiles(ed.stroke, ed.tool === 'remove' ? 'rgba(180,35,24,.5)' : 'rgba(255,255,255,.45)');
    if (ed.selection) rect(ed.selection, '#FF7A45', 'rgba(255,122,69,.12)', 2);
    if (ed.dragRect) rect(ed.dragRect, '#FFD27A', 'rgba(255,210,122,.15)', 2);
    if (ed.paste && ed.ed.clipboard) {
      const [pw, ph] = ed.pasteSize();
      rect({ x: ed.paste.x, y: ed.paste.y, w: pw, h: ph }, '#7FD4FF', 'rgba(127,212,255,.25)', 2);
    }
    if (ed.layers.objects) {
      if (w.spawn) mark(w.spawn[0], w.spawn[1], '#7FD4FF', 3);
      ed.blockheads.forEach((b, i) => mark(b.x, b.y, i === ed.picked ? '#FF7A45' : '#FFFFFF', 3));
    }
    if (ed.tile) mark(ed.tile.x, ed.tile.y, '#FFD27A', 3);
    if (ed.hover) mark(ed.hover.x, ed.hover.y, 'rgba(255,255,255,.8)', 1);
  }

  let down: { x: number; y: number; moved: boolean; pan: boolean; tile: { x: number; y: number } } | null = null;

  function local(e: MouseEvent) {
    const r = (box as HTMLDivElement).getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  /** The tile under the pointer, not wrapped (strokes and rectangles stay continuous across the seam). */
  function rawTile(p: { x: number; y: number }) {
    return tileAt(ed.cam, ed.size, p.x, p.y);
  }

  function pointerdown(e: PointerEvent) {
    const p = local(e);
    const tile = rawTile(p);
    if (e.button === 2 || (e.button === 0 && e.altKey)) {
      const at = ed.at(p.x, p.y);
      void ed.pickBlock(at.x, at.y);
      return;
    }
    if (e.button !== 0) return;
    const drawing = ed.tool === 'pencil' || ed.tool === 'brush' || ed.tool === 'remove';
    const rectTool = ed.tool === 'select' || ed.tool === 'fill';
    down = { ...p, moved: false, pan: !(drawing || rectTool), tile };
    if (drawing) ed.extendStroke(ed.strokePoints(tile, tile));
    if (rectTool) ed.dragRect = ed.rectFrom(tile, tile);
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function pointermove(e: PointerEvent) {
    const p = local(e);
    ed.hover = ed.at(p.x, p.y);
    const tile = rawTile(p);
    if (ed.paste && !down) {
      const [pw, ph] = ed.pasteSize();
      ed.movePaste(tile.x - Math.floor(pw / 2), tile.y - Math.floor(ph / 2));
    }
    if (!down) return;
    if (!down.pan) {
      if (ed.dragRect) ed.dragRect = ed.rectFrom(down.tile, tile);
      else ed.extendStroke(ed.strokePoints(down.tile, tile));
      down.tile = ed.dragRect ? down.tile : tile;
      return;
    }
    const dx = p.x - down.x;
    const dy = p.y - down.y;
    if (!down.moved && Math.abs(dx) + Math.abs(dy) < 4) return;
    down.moved = true;
    ed.pan(dx, dy);
    down.x = p.x;
    down.y = p.y;
  }

  function pointerup(e: PointerEvent) {
    const was = down;
    down = null;
    if (!was) return;
    if (!was.pan) {
      if (ed.dragRect) void ed.finishRect(ed.dragRect);
      else void ed.commitStroke();
      return;
    }
    if (was.moved) return;
    const p = local(e);
    if (ed.tool === 'paste') {
      void ed.placePaste();
      return;
    }
    const tile = ed.at(p.x, p.y);
    void ed.inspect(tile.x, tile.y);
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const p = local(e);
    ed.zoom(e.deltaY < 0 ? 1 : -1, p.x, p.y);
  }

  function keydown(e: KeyboardEvent) {
    const step = e.shiftKey ? 32 : 4;
    const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
    if (ed.paste && moves[e.key]) {
      const [dx, dy] = moves[e.key].map((v) => Math.sign(v)) as [number, number];
      ed.movePaste(ed.paste.x + dx, ed.paste.y + dy);
    } else if (moves[e.key]) ed.panTiles(...moves[e.key]);
    else if (e.key === '+' || e.key === '=') ed.zoom(1);
    else if (e.key === '-') ed.zoom(-1);
    else if (e.key === 'Enter' && ed.paste) void ed.placePaste();
    else if (e.key === 'Enter') {
      const c = ed.at(ed.size.w / 2, ed.size.h / 2);
      void ed.inspect(c.x, c.y);
    } else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class={['map', `t-${ed.tool}`]}
  bind:this={box}
  role="application"
  aria-label={t('edv.mapLabel')}
  tabindex="0"
  onpointerdown={pointerdown}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointerleave={() => (ed.hover = null)}
  oncontextmenu={(e) => e.preventDefault()}
  onwheel={wheel}
  onkeydown={keydown}
>
  <canvas bind:this={canvas} style={`width:${ed.size.w}px;height:${ed.size.h}px`}></canvas>
</div>

<style>
  .map { position: absolute; inset: 0; outline: none; touch-action: none; cursor: crosshair; }
  .map.t-inspect { cursor: default; }
  .map:focus-visible { box-shadow: inset 0 0 0 3px var(--focus); }
  canvas { display: block; image-rendering: pixelated; }
</style>
