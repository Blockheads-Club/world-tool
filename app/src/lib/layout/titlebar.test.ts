import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({
  focus: null as null | ((e: { payload: boolean }) => void),
  resized: null as null | (() => void),
  slow: null as null | ((u: () => void) => void),
  unlisten: vi.fn()
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => true,
    toggleMaximize: async () => {},
    minimize: async () => {},
    close: async () => {},
    onResized: (cb: () => void) => {
      h.resized = cb;
      return new Promise<() => void>((r) => (h.slow = r));
    },
    onFocusChanged: async (cb: (e: { payload: boolean }) => void) => {
      h.focus = cb;
      return h.unlisten;
    }
  })
}));

import TitleBar from './TitleBar.svelte';

afterEach(() => cleanup());

describe('TitleBar', () => {
  it('follows maximize and focus, and unlistens also when a listener arrives after unmount', async () => {
    const r = render(TitleBar, { os: 'windows', ctx: 'Worlds' });
    expect(await screen.findByRole('button', { name: 'Restore window' })).toBeInTheDocument();
    h.focus?.({ payload: false });
    await vi.waitFor(() => expect(screen.getByRole('banner')).toHaveClass('inactive'));
    h.focus?.({ payload: true });
    h.resized?.();
    r.unmount();
    expect(h.unlisten).toHaveBeenCalled();
    const late = vi.fn();
    h.slow?.(late);
    await vi.waitFor(() => expect(late).toHaveBeenCalled());
  });

  it('keeps listeners that arrive while mounted', async () => {
    render(TitleBar, { os: 'linux' });
    const kept = vi.fn();
    h.slow?.(kept);
    await new Promise((r) => setTimeout(r, 0));
    expect(kept).not.toHaveBeenCalled();
    cleanup();
    expect(kept).toHaveBeenCalled();
    await fireEvent.click(document.body);
  });
});
