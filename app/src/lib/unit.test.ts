import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toAppError } from './errors';
import { bytes, clock, day, eta, mmss, pct, when } from './format';
import { detectLang, i18n, LANGUAGES, num, setLang, t } from './i18n.svelte';
import { en } from './messages';
import { kk } from './messages-kk';
import { ru } from './messages-ru';
import { tr } from './messages-tr';
import { effectiveDark, setReduceMotion, setTheme, theme } from './theme.svelte';
import { isAppError } from './types';
import { baseName, copyText, dirName, opId, shortId } from './util';

describe('format', () => {
  beforeEach(() => setLang('en'));
  it('formats sizes', () => {
    expect(bytes(500)).toBe('1 KB');
    expect(bytes(48 * 1024 * 1024)).toBe('48 MB');
    expect(bytes(1.24 * 1024 * 1024)).toBe('1.2 MB');
    expect(bytes(41 * 1024 ** 3)).toBe('41 GB');
    setLang('ru');
    expect(bytes(1.5 * 1024 * 1024)).toBe('1,5 МБ');
  });
  it('formats dates, times and durations', () => {
    const now = Date.UTC(2026, 10, 3, 15, 52);
    expect(when(now - 60_000, now)).toBe('Today 15:51');
    expect(when(now - 86_400_000, now)).toBe('Yesterday 15:52');
    expect(when(Date.UTC(2026, 9, 1, 9, 5), now)).toBe('Oct 1, 2026');
    expect(when(Date.UTC(2026, 9, 1, 9, 5), now, true)).toBe('Oct 1, 2026 09:05');
    expect(day(Date.UTC(2025, 2, 3))).toBe('Mar 3, 2025');
    expect(clock(now)).toBe('15:52');
    expect(mmss(12)).toBe('0:12');
    expect(mmss(75)).toBe('1:15');
    expect(eta(40)).toBe('about 40 s left');
    expect(eta(0)).toBe('about 1 s left');
    expect(eta(130)).toBe('about 2 min left');
    expect(pct(46, 100)).toBe(46);
    expect(pct(5, 0)).toBe(0);
    expect(pct()).toBe(0);
    expect(pct(200, 100)).toBe(100);
  });
});

describe('i18n', () => {
  beforeEach(() => setLang('en'));
  it('interpolates, falls back and switches', () => {
    expect(t('dev.game', { v: '1.7.6' })).toBe('Game 1.7.6');
    expect(t('dev.game')).toBe('Game {v}');
    setLang('tr');
    expect(document.documentElement.lang).toBe('tr');
    setLang('xx');
    expect(i18n.lang).toBe('en');
    i18n.lang = 'zz' as never;
    expect(t('common.back')).toBe('Back');
    expect(t('nope' as never)).toBe('nope');
  });
  it('resolves plurals, also with placeholders inside the cases', () => {
    expect(t('read.count', { n: 1 })).toBe('1 world');
    expect(t('read.count', { n: 5 })).toBe('5 worlds');
    expect(t('tr.after', { n: 7, name: 'Pixel' })).toBe('After copying, Pixel will have 7 worlds');
    setLang('ru');
    expect(t('read.count', { n: 2 })).not.toBe(t('read.count', { n: 5 }));
    i18n.lang = 'ar' as never;
    expect(t('read.count', { n: 2 })).toBe('2 worlds');
  });
  it('formats numbers per language', () => {
    expect(num(1.5)).toBe('1.5');
    expect(num(2, 1)).toBe('2.0');
    setLang('kk');
    expect(num(1.5)).toBe('1,5');
  });
  it('detects the system language', () => {
    expect(LANGUAGES.map((l) => l.label)).toEqual(['English', 'Türkçe', 'Русский', 'Қазақша']);
    expect(detectLang(['ru-RU'])).toBe('ru');
    expect(detectLang(['de', 'kk_KZ'])).toBe('kk');
    expect(detectLang(['fr'])).toBe('en');
  });
  it('keeps every language complete, with the same placeholders and plural cases', () => {
    const holes = (s: string) => [...new Set([...s.matchAll(/\{(\w+)[,}]/g)].map((m) => m[1]))].sort().join();
    const cases = (s: string) =>
      [...s.matchAll(/\{\w+, plural,((?:\s*\w+ \{(?:[^{}]|\{\w+\})*\})+)\s*\}/g)]
        .map((p) => [...p[1].matchAll(/(\w+) \{(?:[^{}]|\{\w+\})*\}/g)].map((m) => m[1]).join())
        .join('|');
    const want: Record<string, string> = { tr: 'one,other', ru: 'one,few,many,other', kk: 'one,other' };
    for (const [code, dict] of Object.entries({ tr, ru, kk })) {
      expect(Object.keys(dict), code).toEqual(Object.keys(en));
      for (const k of Object.keys(en) as (keyof typeof en)[]) {
        expect(dict[k].length, `${code} ${k} empty`).toBeGreaterThan(0);
        expect(holes(dict[k]), `${code} ${k}`).toBe(holes(en[k]));
        if (en[k].includes(', plural,')) expect(cases(dict[k]), `${code} ${k}`).toBe(cases(en[k]).split('|').map(() => want[code]).join('|'));
      }
    }
  });
});

describe('theme', () => {
  it('sets theme and motion on <html>', () => {
    setTheme('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(effectiveDark()).toBe(true);
    setTheme('system');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(effectiveDark()).toBe(false);
    setTheme('light');
    expect(effectiveDark()).toBe(false);
    setReduceMotion(true);
    expect(document.documentElement.dataset.motion).toBe('reduce');
    expect(theme.reduceMotion).toBe(true);
    setReduceMotion(false);
    expect(document.documentElement.hasAttribute('data-motion')).toBe(false);
  });
});

describe('util and errors', () => {
  it('helps with ids, paths and the clipboard', async () => {
    expect(opId('x')).not.toBe(opId('x'));
    expect(opId()).toMatch(/^op-/);
    expect(shortId('a41f000000009c2')).toBe('a41f…09c2');
    expect(shortId('short')).toBe('short');
    expect(baseName('C:\\a\\b.bhworld')).toBe('b.bhworld');
    expect(baseName('/')).toBe('/');
    expect(dirName('/a/b/c')).toBe('/a/b');
    expect(dirName('c')).toBe('c');
    expect(await copyText('x')).toBe(true);
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('no'));
    expect(await copyText('x')).toBe(false);
  });
  it('turns rejections into AppErrors', () => {
    expect(isAppError({ code: 'io' })).toBe(true);
    expect(isAppError(null)).toBe(false);
    expect(toAppError({ code: 'declined' })).toEqual({ code: 'declined' });
    expect(toAppError(new Error('boom'))).toEqual({ code: 'io', detail: 'boom' });
    expect(toAppError('text')).toEqual({ code: 'io', detail: 'text' });
  });
});
