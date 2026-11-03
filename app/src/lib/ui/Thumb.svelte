<script lang="ts">
  import { t } from '../i18n.svelte';

  /** Map preview of a world, or "No preview". */
  let { src, name, large = false }: { src: string | null; name: string; large?: boolean } = $props();
</script>

{#if src}
  <span class={['thumb', large && 'lg']} role="img" aria-label={t('worlds.previewOf', { name })} style:background-image={`url("${src}")`}></span>
{:else}
  <span class={['thumb', 'none', large && 'lg']}>{t('worlds.noPreview')}</span>
{/if}
