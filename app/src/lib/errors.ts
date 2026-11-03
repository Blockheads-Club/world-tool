import type { AppError } from './types';
import { isAppError } from './types';

/** Turn anything a backend call rejected with into an AppError. */
export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return e;
  return { code: 'io', detail: e instanceof Error ? e.message : String(e) };
}
