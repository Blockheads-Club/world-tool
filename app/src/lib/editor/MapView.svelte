<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import { toScreen, visibleBlocks } from './camera';
  import { MACRO, key } from './data';
  import { BlockCache, blockRes, drawBlock, layerKey, type Ctx } from './paint';
  import type { EditorWorld } from '../types';

  /** The map: blocks drawn into cached per-block canvases, composited at the camera's zoom. */
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
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });

  function blockImage(mx: number, my: number, data: Uint8Array, res: number): HTMLCanvasElement {
    const k = `${mx},${my},${res},${layerKey(ed.layers)},${ed.atlas ? 1 : 0}`;
    const hit = cache.get(k);
    if (hit) return hit;
    const c = document.createElement('canvas');
    c.width = c.height = MACRO * res;
    drawBlock(c.getContext('2d') as unknown as Ctx, data, res, ed.catalog!, ed.atlas, ed.layers);
    cache.set(k, c);
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
    const [x0, y0, x1, y1] = visibleBlocks(cam, ed.size, w.width, w.height, 0);
    const side = MACRO * cam.zoom;
    for (let my = y0; my < y1; my++)
      for (let mx = x0; mx < x1; mx++) {
        const p = toScreen(cam, ed.size, mx * MACRO, my * MACRO + MACRO - 1);
        const b = ed.blocks.get(key(mx, my));
        if (b?.state === 1) {
          const img = blockImage(mx, my, b.data as Uint8Array, res);
          ctx.drawImage(img, 0, 0, img.width, img.height, p.x, p.y, side, side);
        } else if (b) {
          // not generated (dim) or damaged (red)
          ctx.fillStyle = b.state === 2 ? 'rgba(180,35,24,.45)' : 'rgba(31,26,23,.35)';
          ctx.fillRect(p.x, p.y, side, side);
        }
      }
    const mark = (x: number, y: number, color: string, width = 2) => {
      const p = toScreen(cam, ed.size, x, y);
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      ctx.strokeRect(p.x, p.y, cam.zoom, cam.zoom);
    };
    if (ed.layers.objects) {
      if (w.spawn) mark(w.spawn[0], w.spawn[1], '#7FD4FF', 3);
      ed.blockheads.forEach((b, i) => mark(b.x, b.y, i === ed.picked ? '#FF7A45' : '#FFFFFF', 3));
    }
    if (ed.tile) mark(ed.tile.x, ed.tile.y, '#FFD27A', 3);
    if (ed.hover) mark(ed.hover.x, ed.hover.y, 'rgba(255,255,255,.8)', 1);
  }

  let down: { x: number; y: number; moved: boolean } | null = null;

  function local(e: MouseEvent) {
    const r = (box as HTMLDivElement).getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function pointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    const p = local(e);
    down = { ...p, moved: false };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function pointermove(e: PointerEvent) {
    const p = local(e);
    ed.hover = ed.at(p.x, p.y);
    if (!down) return;
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
    if (!was || was.moved) return;
    const p = local(e);
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
    if (moves[e.key]) ed.panTiles(...moves[e.key]);
    else if (e.key === '+' || e.key === '=') ed.zoom(1);
    else if (e.key === '-') ed.zoom(-1);
    else if (e.key === 'Enter') {
      const c = ed.at(ed.size.w / 2, ed.size.h / 2);
      void ed.inspect(c.x, c.y);
    } else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="map"
  bind:this={box}
  role="application"
  aria-label={t('edv.mapLabel')}
  tabindex="0"
  onpointerdown={pointerdown}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointerleave={() => (ed.hover = null)}
  onwheel={wheel}
  onkeydown={keydown}
>
  <canvas bind:this={canvas} style={`width:${ed.size.w}px;height:${ed.size.h}px`}></canvas>
</div>

<style>
  .map { position: absolute; inset: 0; outline: none; touch-action: none; cursor: crosshair; }
  .map:focus-visible { box-shadow: inset 0 0 0 3px var(--focus); }
  canvas { display: block; image-rendering: pixelated; }
</style>
