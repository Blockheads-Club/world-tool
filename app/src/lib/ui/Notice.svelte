<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '../icons';
  import Icon from './Icon.svelte';

  interface Props {
    tone?: 'info' | 'warn' | 'bad' | 'ok';
    title: string;
    icon?: IconName;
    /** `alert` for errors and destructive warnings, `status` for results, `note` otherwise. */
    role?: 'alert' | 'status' | 'note';
    children?: Snippet;
    actions?: Snippet;
  }
  let { tone = 'info', title, icon, role = 'note', children, actions }: Props = $props();
  const defaults = { info: 'info', ok: 'check', warn: 'alert', bad: 'alert' } as const;
</script>

<div class={['notice', tone]} {role}>
  <span class="ic"><Icon name={icon ?? defaults[tone]} /></span>
  <div class="nb">
    <b>{title}</b>
    {#if children}<p class="muted">{@render children()}</p>{/if}
    {#if actions}<div class="na">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .notice { display: flex; gap: 14px; align-items: flex-start; padding: 14px 16px; border-radius: 14px; border: 2px solid transparent; }
  .info { background: var(--info-soft); border-color: color-mix(in oklab, var(--info), transparent 65%); }
  .warn { background: var(--warn-soft); border-color: color-mix(in oklab, var(--warn), transparent 60%); }
  .bad { background: var(--bad-soft); border-color: color-mix(in oklab, var(--bad), transparent 60%); }
  .ok { background: var(--ok-soft); border-color: color-mix(in oklab, var(--ok), transparent 60%); }
  .ic { display: flex; margin-top: 2px; }
  .info .ic { color: var(--info); }
  .warn .ic { color: var(--warn); }
  .bad .ic { color: var(--bad); }
  .ok .ic { color: var(--ok); }
  .nb { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .muted { font-size: 14.5px; }
  .na { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; margin-top: 6px; }
</style>
