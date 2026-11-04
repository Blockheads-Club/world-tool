<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';

  /** Asks for a tile position (Go to, Teleport); `onpick` gets a valid one. */
  let { title, action, start, onpick, onclose }: { title: string; action: string; start: [number, number]; onpick: (x: number, y: number) => void; onclose: () => void } = $props();
  const w = $derived(store.editor.world!);
  // svelte-ignore state_referenced_locally
  let x = $state(String(start[0]));
  // svelte-ignore state_referenced_locally
  let y = $state(String(start[1]));
  let bad = $state(false);

  function go(e: SubmitEvent) {
    e.preventDefault();
    const nx = Number(x);
    const ny = Number(y);
    if (!Number.isInteger(nx) || !Number.isInteger(ny) || x.trim() === '' || y.trim() === '' || nx < 0 || ny < 0 || nx >= w.width || ny >= w.height) {
      bad = true;
      return;
    }
    onpick(nx, ny);
  }
</script>

<Dialog {title} icon="target" {onclose}>
  <form id="goto" class="row" onsubmit={go} novalidate>
    <label>X <input class="input mono" inputmode="numeric" bind:value={x} data-autofocus aria-invalid={bad} /></label>
    <label>Y <input class="input mono" inputmode="numeric" bind:value={y} aria-invalid={bad} /></label>
  </form>
  {#if bad}<p class="err" role="alert">{t('edv.goto.invalid', { w: w.width - 1, h: w.height - 1 })}</p>{/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={onclose}>{t('common.cancel')}</Button>
    <Button variant="primary" onclick={() => (document.getElementById('goto') as HTMLFormElement).requestSubmit()}>{action}</Button>
  {/snippet}
</Dialog>

<style>
  .row { display: flex; gap: 14px; }
  label { display: flex; flex-direction: column; gap: 6px; font-weight: 700; flex: 1; }
  .err { color: var(--bad); font-size: 14px; }
</style>
