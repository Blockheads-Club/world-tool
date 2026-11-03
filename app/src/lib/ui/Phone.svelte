<script lang="ts">
  import { t } from '../i18n.svelte';

  /** A phone picture showing the Android screen the user has to answer. */
  let { screen }: { screen: 'allow' | 'backup' | 'restore' } = $props();
</script>

{#if screen === 'allow'}
  <div class="phone" role="img" aria-label={t('phone.allowAria')}>
    <div class="scr dark">
      <div class="box">
        <b>{t('phone.allowTitle')}</b>
        <span class="sm">{t('phone.allowKey')}</span>
        <span class="row sm"><span class="tick" aria-hidden="true"></span>{t('phone.allowAlways')}</span>
        <div class="pf"><span>{t('phone.cancel')}</span><span class="hl">{t('phone.allow')}</span></div>
      </div>
    </div>
  </div>
{:else}
  <div class="phone" role="img" aria-label={screen === 'backup' ? t('phone.backupAria') : t('phone.restoreAria')}>
    <div class="scr">
      <b>{screen === 'backup' ? t('phone.backupTitle') : t('phone.restoreTitle')}</b>
      <span>{screen === 'backup' ? t('phone.backupText') : t('phone.restoreText')}</span>
      <span class="sm grey">{t('phone.optPassword')}</span>
      <span class="pw">{t('phone.encPassword')}</span>
      <div class="pf">
        <span>{screen === 'backup' ? t('phone.dontBackup') : t('phone.dontRestore')}</span>
        <span class="hl">{screen === 'backup' ? t('phone.backupBtn') : t('phone.restoreBtn')}</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .phone { width: 212px; height: 400px; border-radius: 32px; background: #1f1a17; padding: 10px; box-shadow: var(--lift); flex: none; }
  .scr { width: 100%; height: 100%; border-radius: 24px; background: #f4f4f4; color: #1f1a17; display: flex; flex-direction: column; padding: 22px 16px 16px; gap: 10px; font: 13px/1.4 system-ui, sans-serif; overflow: hidden; }
  .scr.dark { background: #2b2b2b; justify-content: center; padding: 12px; }
  .box { background: #f4f4f4; color: #1f1a17; border-radius: 16px; padding: 16px; display: flex; flex-direction: column; gap: 8px; }
  b { font: 700 17px system-ui, sans-serif; }
  .sm { font-size: 12px; }
  .grey { color: #555; }
  .row { display: flex; gap: 6px; align-items: center; }
  .tick { width: 16px; height: 16px; border-radius: 4px; background: #c2410c; flex: none; }
  .pw { border-bottom: 2px solid #1d5fb8; padding: 6px 0; color: #777; }
  .pf { margin-top: auto; display: flex; justify-content: space-between; gap: 6px; font: 700 12px system-ui; color: #1d5fb8; text-transform: uppercase; }
  .box .pf { margin-top: 6px; }
  .pf span { padding: 8px 6px; }
  .pf .hl { outline: 3px solid #ff7a45; outline-offset: 2px; border-radius: 6px; }
</style>
