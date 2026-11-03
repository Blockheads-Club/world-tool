<script lang="ts">
  /** Native checkbox drawn as the design's block checkbox; `mixed` for "some selected". */
  let {
    checked = $bindable(false),
    mixed = false,
    label,
    hideLabel = false,
    onchange
  }: { checked?: boolean; mixed?: boolean; label: string; hideLabel?: boolean; onchange?: (v: boolean) => void } = $props();
  let input: HTMLInputElement | undefined = $state();
  $effect(() => {
    if (input) input.indeterminate = mixed;
  });
</script>

<label class="ck">
  <input type="checkbox" bind:this={input} bind:checked onchange={() => onchange?.(checked)} />
  <span class={['cb', (checked || mixed) && 'on']} aria-hidden="true">
    {#if mixed}<svg viewBox="0 0 24 24"><path d="M6 12h12" /></svg>{:else if checked}<svg viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
  </span>
  <span class:sr={hideLabel}>{label}</span>
</label>

<style>
  .ck { display: inline-flex; gap: 10px; align-items: center; font-weight: 700; cursor: pointer; position: relative; min-height: 32px; }
  input { position: absolute; opacity: 0; width: 22px; height: 22px; margin: 0; cursor: pointer; }
  .cb { width: 22px; height: 22px; border-radius: 6px; border: 2px solid var(--line-strong); background: var(--surface); display: inline-grid; place-items: center; flex: none; color: var(--on-primary); }
  .cb.on { background: var(--primary); border-color: var(--primary-edge); }
  svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 3; stroke-linecap: round; stroke-linejoin: round; }
  input:focus-visible + .cb { outline: 3px solid var(--focus); outline-offset: 2px; }
</style>
