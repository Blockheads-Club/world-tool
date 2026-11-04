/** History step labels in the UI language. */

import { t } from '../i18n.svelte';
import type { Catalog, HistoryLabel } from '../types';
import { contentsName, itemName, tileName } from './names';

/**
 * One detail of a step: block tokens (`t:<tile>`, `t:<tile>:<contents>`, `b:<tile>`), item tokens (`i:<id>`),
 * or a name (blockhead, schematic) shown as it is.
 */
export function part(cat: Catalog | null, v: string | undefined): string {
  if (!v) return '';
  const m = /^([tbi]):(\d+)(?::(\d+))?$/.exec(v);
  if (!m) return v;
  const id = Number(m[2]);
  if (m[1] === 'i') return itemName(cat, id);
  if (m[1] === 'b') return t('edv.backOf', { name: tileName(cat, id) });
  return m[3] ? contentsName(cat, Number(m[3])) : tileName(cat, id);
}

export function stepText(cat: Catalog | null, l: HistoryLabel): string {
  return t(`edv.op.${l.kind}`, { n: l.tiles, a: part(cat, l.a), b: part(cat, l.b) });
}
