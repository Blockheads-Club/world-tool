<script lang="ts">
  import { LANGUAGES, i18n, setLang, t } from '../i18n.svelte';
  import { store } from '../store.svelte';
  import { effectiveDark, setTheme } from '../theme.svelte';
  import Button from '../ui/Button.svelte';
  import Check from '../ui/Check.svelte';
  import LegalDialog from './LegalDialog.svelte';

  let ok = $state(false);
  let legal = $state(false);
  let dark = $state(effectiveDark());

  function lang(code: string) {
    setLang(code);
    void store.saveSettings({ lang: i18n.lang });
  }

  function toggleDark() {
    dark = !dark;
    setTheme(dark ? 'dark' : 'light');
    void store.saveSettings({ theme: dark ? 'dark' : 'light' });
  }
</script>

<div class="app fr">
  <section class="card box" aria-labelledby="fr-t">
    <div class="top">
      <img src="/wt-icon.svg" alt="" width="72" height="72" />
      <div class="tt"><span class="hint">Blockheads Club</span><h1 id="fr-t">{t('fr.title')}</h1></div>
      <select class="sel" aria-label={t('set.language')} value={i18n.lang} onchange={(e) => lang(e.currentTarget.value)}>
        {#each LANGUAGES as l (l.code)}<option value={l.code}>{l.label}</option>{/each}
      </select>
    </div>
    <p class="muted lead">{t('fr.lead')}</p>
    <div class="card before">
      <b>{t('fr.before')}</b>
      <ul class="ol">
        <li>{t('fr.own')}</li>
        <li><b>{t('fr.noUpload')}</b> {t('fr.noUpload2')}</li>
        <li>{t('fr.backupFirst')}</li>
        <li>{t('fr.unofficial')}</li>
      </ul>
      <button class="link" onclick={() => (legal = true)}>{t('fr.legal')}</button>
    </div>
    <Check bind:checked={ok} label={t('fr.understand')} />
    <div class="kpi">
      <Button variant="primary" size="lg" icon="chevr" disabled={!ok} onclick={() => void store.saveSettings({ first_run_done: true })}>{t('common.continue')}</Button>
      <Button variant="ghost" size="lg" icon={dark ? 'sun' : 'moon'} onclick={toggleDark}>{dark ? t('fr.light') : t('fr.dark')}</Button>
    </div>
  </section>
</div>
{#if legal}<LegalDialog onclose={() => (legal = false)} />{/if}

<style>
  .fr { overflow: auto; display: grid; place-items: center; padding: 28px; }
  .box { max-width: 720px; width: 100%; padding: 32px; display: flex; flex-direction: column; gap: 16px; }
  .top { display: flex; gap: 16px; align-items: center; flex-wrap: wrap; }
  .tt { flex: 1; min-width: 0; }
  .tt .hint { font-weight: 700; }
  h1 { font-size: 30px; }
  .lead { font-size: 17px; }
  .before { padding: 16px 18px; background: var(--surface-2); display: flex; flex-direction: column; gap: 10px; }
  .before .ol { padding-left: 20px; }
  .link { align-self: flex-start; }
</style>
