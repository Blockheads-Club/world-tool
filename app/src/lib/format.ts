import { i18n, num, t } from './i18n.svelte';

const MB = 1024 * 1024;

/** "48 MB", "1.2 MB", "41 GB", "820 KB" in the language's number format. */
export function bytes(b: number): string {
  if (b >= 1024 * MB) return t('unit.gb', { n: num(round(b / (1024 * MB))) });
  if (b >= MB) return t('unit.mb', { n: num(round(b / MB)) });
  return t('unit.kb', { n: num(Math.max(1, Math.round(b / 1024))) });
}

/** One decimal below 10, whole numbers above. */
function round(x: number): number {
  return x < 10 ? Math.round(x * 10) / 10 : Math.round(x);
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** "Today 15:41", "Yesterday 21:10" or "2 Nov 2026" (with the time when `withTime`). */
export function when(ms: number, now = Date.now(), withTime = false): string {
  const d = new Date(ms);
  const time = new Intl.DateTimeFormat(i18n.lang, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(d);
  if (sameDay(d, new Date(now))) return t('time.today', { time });
  if (sameDay(d, new Date(now - 86_400_000))) return t('time.yesterday', { time });
  const date = day(ms);
  return withTime ? `${date} ${time}` : date;
}

/** "2 Nov 2026". */
export function day(ms: number): string {
  return new Intl.DateTimeFormat(i18n.lang, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(ms));
}

/** "HH:MM". */
export function clock(ms: number): string {
  return new Intl.DateTimeFormat(i18n.lang, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(new Date(ms));
}

/** Seconds as "0:12". */
export function mmss(s: number): string {
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
}

/** "about 40 s left" / "about 2 min left". */
export function eta(s: number): string {
  return s < 60 ? t('time.etaS', { n: Math.max(1, Math.round(s)) }) : t('time.etaM', { n: Math.round(s / 60) });
}

/** Percent 0–100 of done / total. */
export function pct(done = 0, total = 0): number {
  return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
}

/** Display helpers for values the backend may not know: "-" / 0 / "". */
export const dash = (v: string | null | undefined): string => v ?? '-';
export const zero = (v: number | null | undefined): number => v ?? 0;
export const blank = (v: string | null | undefined): string => v ?? '';
