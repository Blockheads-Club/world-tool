/** Display names of ids (English names from the catalog; unknown ids say so). */

import { t } from '../i18n.svelte';
import type { Catalog } from '../types';
import { pretty } from './data';

export function tileName(cat: Catalog | null, id: number): string {
  const n = cat?.tiles[id]?.name;
  return n && n !== 'none' ? pretty(n) : t('edv.unknown', { id });
}

export function contentsName(cat: Catalog | null, id: number): string {
  if (id === 0) return t('edv.nothing');
  const n = cat?.contents[id]?.name;
  return n ? pretty(n) : t('edv.unknown', { id });
}

export function itemName(cat: Catalog | null, id: number): string {
  const n = cat?.items.find((i) => i.id === id)?.name;
  return n ? pretty(n) : t('edv.unknown', { id });
}
