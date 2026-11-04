/**
 * Display names of ids: tiles, contents and items, in the UI language (tr/ru/kk tables) with the game's English
 * ids as fallback. Search matches the translated name and the English one.
 */

import { i18n, t } from '../i18n.svelte';
import type { Brush, Catalog, IdInfo } from '../types';
import { pretty } from './data';
import { kk } from './names-kk';
import { ru } from './names-ru';
import { tr } from './names-tr';

type Table = { tiles: Record<string, string>; contents: Record<string, string>; items: Record<string, string> };
const LOCAL: Partial<Record<string, Table>> = { tr, ru, kk };

function local(section: keyof Table, k: string): string | undefined {
  return LOCAL[i18n.lang]?.[section][k];
}

export function tileName(cat: Catalog | null, id: number): string {
  const n = cat?.tiles[id]?.name;
  return n && n !== 'none' ? (local('tiles', n) ?? pretty(n)) : t('edv.unknown', { id });
}

export function contentsName(cat: Catalog | null, id: number): string {
  if (id === 0) return t('edv.nothing');
  const n = cat?.contents[id]?.name;
  return n ? (local('contents', n) ?? pretty(n)) : t('edv.unknown', { id });
}

export function itemName(cat: Catalog | null, id: number): string {
  const n = cat?.items.find((i) => i.id === id)?.name;
  return n ? (local('items', n) ?? pretty(n)) : t('edv.unknown', { id });
}

/** The name of a palette block (an ore names its contents). */
export function brushName(cat: Catalog | null, b: Brush): string {
  return b.contents ? contentsName(cat, b.contents) : tileName(cat, b.tile);
}

/** The picture of an item: its Items atlas cell, or for block items the tile's TileMap cell. */
export function itemInfo(cat: Catalog, id: number): { info: IdInfo | undefined; items: boolean } {
  const i = cat.items.find((x) => x.id === id);
  if (!i || i.cell || i.tile === undefined) return { info: i, items: true };
  return { info: cat.tiles[i.tile], items: false };
}

/** True if `query` (lower case) is part of the shown name or of the English id/name. */
export function nameMatches(shown: string, english: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  return !q || shown.toLowerCase().includes(q) || english.replace(/_/g, ' ').toLowerCase().includes(q);
}
