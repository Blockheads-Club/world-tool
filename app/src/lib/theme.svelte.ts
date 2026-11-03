import type { ThemeChoice } from './types';

/** Theme and motion live on <html>: `data-theme` (absent = follow the OS) and `data-motion="reduce"`. */
export const theme = $state<{ choice: ThemeChoice; reduceMotion: boolean }>({ choice: 'system', reduceMotion: false });

export function setTheme(choice: ThemeChoice) {
  theme.choice = choice;
  if (choice === 'system') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', choice);
}

/** On: no animations even when the OS allows them. Off: the OS setting decides. */
export function setReduceMotion(on: boolean) {
  theme.reduceMotion = on;
  if (on) document.documentElement.setAttribute('data-motion', 'reduce');
  else document.documentElement.removeAttribute('data-motion');
}

/** Whether the UI is dark right now (explicit choice, else the OS preference). */
export function effectiveDark(): boolean {
  return theme.choice === 'dark' || (theme.choice === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
}
