<script lang="ts">
  import { t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';

  /** Go to coordinates (G). */
  const ed = $derived(store.editor);
  let x = $state(String(Math.floor(store.editor.cam.x)));
  let y = $state(String(Math.floor(store.editor.cam.y)));
  let bad = $state(false);

  function go(e: SubmitEvent) {
    e.preventDefault();
    const w = ed.world!;
    const nx = Number(x);
    const ny = Number(y);
    if (!Number.isInteger(nx) || !Number.isInteger(ny) || x.trim() === '' || y.trim() === '' || nx < 0 || ny < 0 || nx >= w.width || ny >= w.height) {
      bad = true;
      return;
    }
    ed.goToOpen = false;
    ed.goTo(nx, ny);
  }
</script>

<Dialog title={t('edv.goto.title')} icon="target" onclose={() => (ed.goToOpen = false)}>
  <form id="goto" class="row" onsubmit={go} novalidate>
    <label>X <input class="input mono" inputmode="numeric" bind:value={x} data-autofocus aria-invalid={bad} /></label>
    <label>Y <input class="input mono" inputmode="numeric" bind:value={y} aria-invalid={bad} /></label>
  </form>
  {#if bad}<p class="err" role="alert">{t('edv.goto.invalid', { w: ed.world!.width - 1, h: ed.world!.height - 1 })}</p>{/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (ed.goToOpen = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" onclick={() => (document.getElementById('goto') as HTMLFormElement).requestSubmit()}>{t('edv.goto.go')}</Button>
  {/snippet}
</Dialog>

<style>
  .row { display: flex; gap: 14px; }
  label { display: flex; flex-direction: column; gap: 6px; font-weight: 700; flex: 1; }
  .err { color: var(--bad); font-size: 14px; }
</style>
