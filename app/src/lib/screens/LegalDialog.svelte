<script lang="ts">
  import { t } from '../i18n.svelte';
  import Button from '../ui/Button.svelte';
  import Dialog from '../ui/Dialog.svelte';

  let { onclose, licences = false }: { onclose: () => void; licences?: boolean } = $props();
</script>

<Dialog title={licences ? t('legal.licencesTitle') : t('legal.title')} icon={licences ? 'file' : 'shield'} {onclose}>
  {#if licences}
    <p>{t('legal.licences1')}</p>
    <ul class="ol mono">
      <li>Tauri · Apache-2.0 / MIT</li>
      <li>Svelte, SvelteKit · MIT</li>
      <li>Bricolage Grotesque, Atkinson Hyperlegible Next, Onest, Golos Text, JetBrains Mono · OFL-1.1</li>
    </ul>
    <p class="hint">{t('legal.licences2')}</p>
  {:else}
    <p>{t('legal.p1')}</p>
    <p>{t('legal.p2')}</p>
    <p>{t('legal.p3')}</p>
    <p class="hint">{t('legal.p4')}</p>
  {/if}
  {#snippet footer()}<Button variant="primary" onclick={onclose}>{t('common.close')}</Button>{/snippet}
</Dialog>
