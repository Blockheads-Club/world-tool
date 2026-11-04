<script lang="ts">
  import type { IdInfo } from '../types';
  import { CELL, UNKNOWN, colorOf } from './paint';

  /** A small picture of a tile, contents or item: its atlas cell when the game files are there, else its colour. */
  let { info, atlas, size, label }: { info: IdInfo | undefined; atlas: CanvasImageSource | null; size: number; label: string } = $props();
  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    const ctx = (canvas as HTMLCanvasElement).getContext('2d') as CanvasRenderingContext2D;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, size, size);
    if (atlas && info?.cell) ctx.drawImage(atlas, info.cell[0] * CELL, info.cell[1] * CELL, CELL, CELL, 0, 0, size, size);
    else {
      ctx.fillStyle = colorOf(info) ?? UNKNOWN;
      ctx.fillRect(0, 0, size, size);
    }
  });
</script>

<span class="sw" role="img" aria-label={label} style:width="{size}px" style:height="{size}px"><canvas bind:this={canvas} width={size} height={size} aria-hidden="true"></canvas></span>

<style>
  .sw { display: inline-block; border-radius: 6px; overflow: hidden; flex: none; border: 1px solid var(--line-strong); }
  canvas { display: block; width: 100%; height: 100%; image-rendering: pixelated; }
</style>
