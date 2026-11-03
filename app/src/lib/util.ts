/** Copy text to the clipboard; false when the platform refuses. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

let n = 0;
/** A fresh operation id for `cancel`. */
export function opId(prefix = 'op'): string {
  n += 1;
  return `${prefix}-${Date.now().toString(36)}-${n}`;
}

/** "a41f…09c2" for long ids. */
export function shortId(id: string): string {
  return id.length <= 9 ? id : `${id.slice(0, 4)}…${id.slice(-4)}`;
}

/** The last part of a path (both separators). */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** The folder part of a path. */
export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i > 0 ? path.slice(0, i) : path;
}
