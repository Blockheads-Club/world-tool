<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { t } from '../i18n.svelte';
  import type { IconName } from '../icons';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  /**
   * ConfirmDialog / any modal: traps focus, returns it to the opener, Esc and the scrim close it (unless `busy`).
   * `alert` makes it an alertdialog (destructive actions and errors).
   */
  interface Props {
    title: string;
    onclose: () => void;
    icon?: IconName;
    tone?: 'p' | 'ok' | 'bad' | 'warn' | 'info';
    alert?: boolean;
    /** While true the dialog cannot be dismissed (an operation runs). */
    busy?: boolean;
    wide?: boolean;
    children?: Snippet;
    footer?: Snippet;
  }
  let { title, onclose, icon, tone = 'p', alert = false, busy = false, wide = false, children, footer }: Props = $props();
  const uid = $props.id();
  let box: HTMLElement | undefined = $state();

  const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';

  $effect(() => {
    const opener = document.activeElement as HTMLElement | null;
    tick().then(() => (box?.querySelector<HTMLElement>('[data-autofocus]') ?? box?.querySelector<HTMLElement>(FOCUSABLE))?.focus());
    return () => opener?.focus?.();
  });

  function close() {
    if (!busy) onclose();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
      return;
    }
    if (e.key !== 'Tab' || !box) return;
    const items = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="scrim" onkeydown={keydown} onmousedown={(e) => e.target === e.currentTarget && close()}>
  <section class={['dlg', wide && 'wide']} role={alert ? 'alertdialog' : 'dialog'} aria-modal="true" aria-labelledby="{uid}-t" bind:this={box}>
    <div class="dh">
      {#if icon}<span class="res"><span class={['ri', tone]}><Icon name={icon} size={26} /></span></span>{/if}
      <h2 id="{uid}-t">{title}</h2>
      {#if !busy}<span class="x"><Button variant="ghost" size="sm" icon="x" label={t('common.close')} onclick={close} /></span>{/if}
    </div>
    {#if children}<div class="db">{@render children()}</div>{/if}
    {#if footer}<div class="df">{@render footer()}</div>{/if}
  </section>
</div>

<style>
  .scrim { position: fixed; inset: var(--titlebar-h, 36px) 0 0 0; z-index: 50; background: var(--scrim); display: grid; place-items: center; padding: 24px; }
  .dlg { background: var(--surface); border: 2px solid var(--line-strong); border-radius: var(--r-xl); box-shadow: var(--lift); width: 100%; max-width: 620px; display: flex; flex-direction: column; max-height: calc(100vh - var(--titlebar-h, 36px) - 48px); overflow: hidden; animation: land var(--dur-move) var(--ease-block); }
  .dlg.wide { max-width: 760px; }
  @keyframes land { from { transform: translateY(12px) scale(0.98); opacity: 0; } }
  .dh { display: flex; gap: 14px; align-items: center; padding: 22px 16px 8px 24px; flex: none; }
  .dh h2 { font-size: 22px; flex: 1; min-width: 0; }
  .x { align-self: flex-start; }
  .db { padding: 8px 24px; display: flex; flex-direction: column; gap: 12px; overflow: auto; min-height: 0; flex: 1 1 auto; }
  .db > :global(*) { flex: none; }
  .df { display: flex; justify-content: flex-end; align-items: center; gap: 10px; flex-wrap: wrap; padding: 16px 24px 22px; border-top: 2px solid var(--line); margin-top: 8px; flex: none; }
</style>
