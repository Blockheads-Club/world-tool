import { render, waitFor } from '@testing-library/svelte';
import { expect } from 'vitest';
import App from './App.svelte';
import { GameFilesFlow } from './flows/gamefiles.svelte';
import { ImportFlow } from './flows/import.svelte';
import { ReadFlow } from './flows/read.svelte';
import { TransferFlow } from './flows/transfer.svelte';
import { UpdateFlow } from './flows/update.svelte';
import { setLang } from './i18n.svelte';
import { MockBackend, type MockOptions } from './mock-api';
import { store } from './store.svelte';
import { setReduceMotion, setTheme } from './theme.svelte';
import { toasts } from './ui/toast.svelte';

/** Put the shared store back to its start-up state (tests share one module instance). */
export function resetStore() {
  store.dispose();
  Object.assign(store, {
    info: { version: '', os: 'windows' },
    settings: null,
    booted: false,
    screen: 'devices',
    guide: 'usb',
    devices: [],
    scanning: false,
    worlds: null,
    game: null,
    shown: null,
    editing: null,
    dragging: null,
    savedEdit: null,
    restoreId: null
  });
  store.read = new ReadFlow(store);
  store.transfer = new TransferFlow(store);
  store.importer = new ImportFlow(store);
  store.gameFiles = new GameFilesFlow(store);
  store.update = new UpdateFlow(store);
  toasts.items = [];
  setLang('en');
  setTheme('system');
  setReduceMotion(false);
}

/** Fresh mock (tick 0) + store, App rendered and booted; optionally tweak the mock before boot. */
export async function setup(opts: MockOptions & { before?: (m: MockBackend) => void } = {}) {
  resetStore();
  const mock = new MockBackend({ tick: 0, now: Date.now(), ...opts });
  opts.before?.(mock);
  const r = render(App);
  await store.init(mock, ['en-US']);
  await waitFor(() => expect(store.devices.length + (opts.scenario === 'empty' ? 1 : 0)).toBeGreaterThan(0));
  return { mock, ...r };
}
