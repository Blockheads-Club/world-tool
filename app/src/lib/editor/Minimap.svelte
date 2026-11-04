<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import { viewTiles } from './camera';
  import { OVERVIEW_SCALE } from './flow.svelte';

  /** The minimap: a window of the plain-colour overview around the view; click moves the view there. Props: the
   * world size in tiles. */
  let { width, height }: { width: number; height: number } = $props();
  const ed = $derived(store.editor);
  const W = 216;
  const H = 132;
  let canvas: HTMLCanvasElement | undefined = $state();
  let full: HTMLCanvasElement | null = null;
  let fullOf: unknown = null;

  /** Tiles the minimap spans: six views wide, at least 256 tiles, at most the world. */
  const span = $derived.by(() => {
    const v = viewTiles(ed.cam, ed.size);
    const sw = Math.min(width, Math.max(256, v.w * 6));
    return { w: sw, h: (sw * H) / W };
  });
  const origin = $derived({ x: ed.cam.x - span.w / 2, y: ed.cam.y + span.h / 2 });
  const view = $derived.by(() => {
    const v = viewTiles(ed.cam, ed.size);
    return {
      left: ((ed.cam.x - v.w / 2 - origin.x) / span.w) * 100,
      top: ((origin.y - (ed.cam.y + v.h / 2)) / span.h) * 100,
      width: (v.w / span.w) * 100,
      height: (v.h / span.h) * 100
    };
  });

  $effect(() => {
    const o = ed.overview;
    const ctx = canvas?.getContext('2d');
    void [origin, span];
    if (!ctx || !o) return;
    if (fullOf !== o) {
      full = document.createElement('canvas');
      full.width = o.width;
      full.height = o.height;
      (full.getContext('2d') as CanvasRenderingContext2D).putImageData(new ImageData(new Uint8ClampedArray(o.pixels), o.width, o.height), 0, 0);
      fullOf = o;
    }
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, W, H);
    const top = height - origin.y;
    ctx.drawImage(full as HTMLCanvasElement, origin.x / OVERVIEW_SCALE, top / OVERVIEW_SCALE, span.w / OVERVIEW_SCALE, span.h / OVERVIEW_SCALE, 0, 0, W, H);
  });

  function click(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const fx = (e.clientX - r.left) / Math.max(r.width, 1);
    const fy = (e.clientY - r.top) / Math.max(r.height, 1);
    ed.goTo(Math.floor(origin.x + fx * span.w), Math.floor(origin.y - fy * span.h), false);
  }
</script>

<div class="float mini">
  <button class="mm" type="button" aria-label={t('edv.minimap')} title={t('edv.minimap')} onclick={click}>
    <canvas bind:this={canvas} width={W} height={H}></canvas>
    <span class="vp" style={`left:${view.left}%;top:${view.top}%;width:${view.width}%;height:${view.height}%`}></span>
  </button>
</div>

<style>
  .mini { right: 12px; bottom: 12px; width: 216px; padding: 6px; }
  .mm { display: block; width: 100%; aspect-ratio: 72 / 44; border-radius: 6px; position: relative; overflow: hidden; padding: 0; border: 0; background: #8fcbea; cursor: pointer; }
  canvas { width: 100%; height: 100%; display: block; image-rendering: pixelated; }
  .vp { position: absolute; outline: 2px solid #ff7a45; background: rgba(255, 255, 255, 0.12); pointer-events: none; }
</style>
