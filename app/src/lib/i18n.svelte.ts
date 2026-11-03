import { en, type Key } from './messages';
import { kk } from './messages-kk';
import { ru } from './messages-ru';
import { tr } from './messages-tr';
import type { Lang } from './types';

/** The app's languages, each named in itself. */
export const LANGUAGES: readonly { code: Lang; label: string }[] = [
  { code: 'en', label: 'English' },
  { code: 'tr', label: 'Türkçe' },
  { code: 'ru', label: 'Русский' },
  { code: 'kk', label: 'Қазақша' }
];

const dicts: Record<string, Record<Key, string>> = { en, tr, ru, kk };

/** The first of the system's preferred languages (`ru-RU`, `kk_KZ`, `tr`…) that the app has, else English. */
export function detectLang(preferred: readonly string[]): Lang {
  for (const tag of preferred) {
    const code = tag.toLowerCase().split(/[-_]/)[0];
    if (code in dicts) return code as Lang;
  }
  return 'en';
}

export const i18n = $state<{ lang: Lang }>({ lang: 'en' });

export function setLang(code: string) {
  i18n.lang = (code in dicts ? code : 'en') as Lang;
  document.documentElement.lang = i18n.lang;
}

export type Params = Record<string, string | number>;

/** `n` in the language's digits and decimal mark (`1.5` / `1,5`), without thousands grouping. */
export function num(n: number, digits?: number): string {
  const opts = digits === undefined ? { maximumFractionDigits: 20 } : { minimumFractionDigits: digits, maximumFractionDigits: digits };
  return new Intl.NumberFormat(i18n.lang, { useGrouping: false, ...opts }).format(n);
}

/** Resolve `{n, plural, one {# world} other {# worlds}}` with the language's plural rules. */
function plural(s: string, params: Params): string {
  return s.replace(/\{(\w+), plural,((?:\s*\w+ \{(?:[^{}]|\{\w+\})*\})+)\s*\}/g, (_, k: string, body: string) => {
    const n = Number(params[k]);
    const cases = Object.fromEntries([...body.matchAll(/(\w+) \{((?:[^{}]|\{\w+\})*)\}/g)].map((m) => [m[1], m[2]]));
    const rule = new Intl.PluralRules(i18n.lang).select(n);
    return (cases[rule] ?? cases.other).replace(/#/g, num(n));
  });
}

/** Translate; `{name}` placeholders are filled from `params`. Unknown keys fall back to English, then the key. */
export function t(key: Key, params: Params = {}): string {
  const s = plural(dicts[i18n.lang]?.[key] ?? en[key] ?? key, params);
  return s.replace(/\{(\w+)\}/g, (_, k) => {
    const v = params[k];
    return v === undefined ? `{${k}}` : typeof v === 'number' ? num(v) : v;
  });
}
