<script lang="ts">
  import { t } from '../i18n.svelte';
  import Icon from './Icon.svelte';

  /** StepIndicator: steps before `current` are done; `current` is 0-based. */
  let { steps, current }: { steps: string[]; current: number } = $props();
</script>

<nav class="steps" aria-label={t('common.steps')}>
  {#each steps as s, i (s)}
    {#if i > 0}<span class="sep" aria-hidden="true"></span>{/if}
    <span class={['s', i < current && 'done', i === current && 'cur']} aria-current={i === current ? 'step' : undefined}>
      <i>{#if i < current}<Icon name="check" size={14} /><span class="sr">{t('common.stepDone')}</span>{:else}{i + 1}{/if}</i>
      {`${s}`}
    </span>
  {/each}
</nav>

<style>
  .steps { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; font: 700 13.5px var(--f-body); color: var(--text-3); }
  .s { display: inline-flex; gap: 8px; align-items: center; padding: 4px 10px 4px 4px; border-radius: 999px; }
  i { width: 24px; height: 24px; border-radius: 50%; display: grid; place-items: center; font: 800 12.5px var(--f-body); font-style: normal; background: var(--surface-3); color: var(--text-2); flex: none; }
  .done i { background: var(--ok); color: #fff; }
  .cur { background: var(--primary-soft); color: var(--primary-ink); }
  .cur i { background: var(--primary); color: var(--on-primary); }
  .sep { width: 18px; height: 2px; background: var(--line-strong); }
</style>
