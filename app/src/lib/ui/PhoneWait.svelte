<script lang="ts">
  import type { Snippet } from 'svelte';
  import { mmss } from '../format';
  import { t } from '../i18n.svelte';
  import Badge from './Badge.svelte';
  import Phone from './Phone.svelte';

  /** PhoneConfirm: "Look at your phone" panel with the phone picture, timer and actions (polite live region). */
  interface Props {
    screen: 'backup' | 'restore';
    title: string;
    text: string;
    elapsed: number;
    /** Which of the phone's three confirmations of a copy or restore this is (1–3); none for a single one. */
    step?: number;
    before?: Snippet;
    actions?: Snippet;
  }
  let { screen, title, text, elapsed, step, before, actions }: Props = $props();
</script>

<div class="card wait" role="status" aria-live="polite">
  <Phone {screen} />
  <div class="wtx">
    <span class="tag"><Badge tone="primary" icon="phone">{step ? t('phone.actionStep', { n: step, total: 3 }) : t('phone.action')}</Badge></span>
    {@render before?.()}
    <h2 class="big">{title}</h2>
    <p class="muted">{text}</p>
    <p class="row"><span class="spin"></span><span>{t('phone.waiting')} <span class="mono">{mmss(elapsed)}</span></span></p>
    {#if actions}<div class="kpi">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .wait { display: flex; gap: 36px; align-items: center; justify-content: center; flex-wrap: wrap; padding: 24px; }
  .wtx { max-width: 460px; display: flex; flex-direction: column; gap: 12px; }
  .tag { align-self: flex-start; }
  .row { display: flex; gap: 10px; align-items: center; }
</style>
