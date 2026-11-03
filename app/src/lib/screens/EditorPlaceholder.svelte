<script lang="ts">
  import { t } from '../i18n.svelte';
  import Header from '../layout/Header.svelte';
  import { store } from '../store.svelte';
  import Button from '../ui/Button.svelte';
  import Empty from '../ui/Empty.svelte';

  /** Phase 2 brings the world editor; for now Worlds → Edit ends here (after the game files step). */
  const name = $derived(store.editing?.name ?? '');
</script>

<Header title={t('ctx.editor')} sub={name} />
<div class="main">
  <Empty icon="pencil" title={t('ed.soonTitle')} text={t('ed.soonText', { name })}>
    <Button variant="primary" icon="back" onclick={() => store.go('worlds')}>{t('ed.back')}</Button>
    <Button icon="sliders" onclick={() => store.go('settings')}>{t('ed.gameFiles')}</Button>
  </Empty>
</div>
