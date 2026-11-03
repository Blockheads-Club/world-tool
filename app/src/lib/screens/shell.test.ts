import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '../i18n.svelte';
import { store } from '../store.svelte';
import { resetStore, setup } from '../test-utils';
import type { AppError } from '../types';

afterEach(() => cleanup());

describe('First run', () => {
  it('needs “I understand”, switches language and dark mode, shows the legal notice', async () => {
    await setup({ scenario: 'fresh' });
    expect(screen.getByRole('heading', { name: 'Welcome to BH World Tool' })).toBeInTheDocument();
    expect(screen.queryByRole('navigation', { name: 'Main' })).toBeNull();
    const go = screen.getByRole('button', { name: 'Continue' });
    expect(go).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Read the full legal notice' }));
    const dlg = await screen.findByRole('dialog', { name: 'Legal notice' });
    await fireEvent.click(within(dlg).getAllByRole('button', { name: 'Close' })[1]);
    await fireEvent.click(screen.getByRole('button', { name: 'Dark mode' }));
    expect(document.documentElement.dataset.theme).toBe('dark');
    await fireEvent.click(screen.getByRole('button', { name: 'Light mode' }));
    expect(document.documentElement.dataset.theme).toBe('light');
    await fireEvent.change(screen.getByRole('combobox', { name: 'Language' }), { target: { value: 'ru' } });
    expect(i18n.lang).toBe('ru');
    await fireEvent.change(screen.getByRole('combobox'), { target: { value: 'en' } });
    await fireEvent.click(screen.getByLabelText('I understand'));
    await fireEvent.click(go);
    expect(await screen.findByRole('navigation', { name: 'Main' })).toBeInTheDocument();
  });
});

describe('Shell', () => {
  it('boots with a spinner, uses the saved language, title bar context per screen', async () => {
    resetStore();
    const { default: App } = await import('../App.svelte');
    render(App);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    const { MockBackend } = await import('../mock-api');
    const m = new MockBackend({ tick: 0 });
    m.settings.lang = 'kk';
    m.settings.auto_update = false;
    await store.init(m);
    expect(i18n.lang).toBe('kk');
    store.dispose();
    store.dispose();
  });

  it('title bar: OS buttons, context, window controls', async () => {
    await setup();
    const bar = screen.getByRole('banner', { name: 'Window title bar' });
    for (const s of ['read', 'transfer', 'worlds', 'backups', 'activity', 'settings', 'editor', 'devices'] as const) {
      if (s === 'read') store.read.start(store.devices[0]);
      else if (s === 'transfer') store.transfer.start();
      else store.screen = s;
      await waitFor(() => expect(bar).toBeInTheDocument());
    }
    expect(within(bar).queryByText(/·/)).toBeNull();
    store.screen = 'activity';
    expect(await within(bar).findByText('· Activity')).toBeInTheDocument();
    for (const name of ['Minimize window', 'Maximize window', 'Close window']) await fireEvent.click(within(bar).getByRole('button', { name }));
    store.info.os = 'linux';
    await waitFor(() => expect(bar.classList.contains('linux')).toBe(true));
    for (const name of ['Minimize window', 'Maximize window', 'Close window']) await fireEvent.click(within(bar).getByRole('button', { name }));
    store.info.os = 'macos';
    await waitFor(() => expect(within(bar).queryByRole('button', { name: 'Close window' })).toBeNull());
  });

  it('shows the rest of the designed error dialogs', async () => {
    await setup();
    const show = async (error: AppError, retry?: () => void) => {
      store.shown = null;
      await waitFor(() => expect(document.querySelector('.dlg')).toBeNull());
      store.shown = { error, retry };
      return waitFor(() => {
        const d = document.querySelector<HTMLElement>('.dlg');
        if (!d) throw new Error('no dialog');
        return d;
      });
    };
    let d = await show({ code: 'game_not_installed', device: 'moto g(7)' });
    expect(d).toHaveAccessibleName('The Blockheads isn’t installed on moto g(7)');
    await fireEvent.click(within(d).getAllByRole('button', { name: 'Close' })[1]);
    d = await show({ code: 'unsupported_version', device: 'Redmi Note 9', version: '1.6.3' }, () => {});
    expect(d).toHaveTextContent('The Blockheads 1.6.3');
    await fireEvent.click(within(d).getByRole('button', { name: 'Try again' }));
    d = await show({ code: 'unsupported_version' });
    expect(d).toHaveTextContent('On the phone');
    expect(d).toHaveTextContent('The Blockheads ?');
    const retry = vi.fn();
    d = await show({ code: 'wrong_password', attempt: 1 }, retry);
    expect(d).toHaveTextContent('Doesn’t match · try 2');
    await fireEvent.input(within(d).getByLabelText('Backup password'), { target: { value: 'pw' } });
    expect(store.read.password).toBe('pw');
    await fireEvent.click(within(d).getByRole('button', { name: 'Try again' }));
    expect(retry).toHaveBeenCalled();
    d = await show({ code: 'wrong_password' });
    expect(d).toHaveTextContent('try 2');
    await fireEvent.click(within(d).getByRole('button', { name: 'Cancel' }));
    for (const [label, expected] of [['Try over Wi-Fi', 'wifi'], ['Use root', 'root']] as const) {
      d = await show({ code: 'backup_empty', detail: 'empty backup' });
      await fireEvent.click(within(d).getByLabelText(new RegExp(label)));
      await fireEvent.click(within(d).getByRole('button', { name: 'Continue' }));
      expect(store.guide).toBe(expected);
    }
    d = await show({ code: 'backup_empty' });
    await fireEvent.click(within(d).getByRole('button', { name: 'Copy details' }));
    await fireEvent.click(within(d).getByLabelText(/Use another device/));
    await fireEvent.click(within(d).getByRole('button', { name: 'Continue' }));
    expect(store.screen).toBe('devices');
    d = await show({ code: 'backup_empty' });
    await fireEvent.click(within(d).getByRole('button', { name: 'Cancel' }));
    d = await show({ code: 'disconnected' });
    expect(d).toHaveTextContent('the phone disconnected at 0 %');
    expect(within(d).queryByText('Your backup is safe')).toBeNull();
    await fireEvent.click(within(d).getByRole('button', { name: 'Later' }));
    d = await show({ code: 'disk_full' });
    await fireEvent.click(within(d).getByRole('button', { name: 'Cancel' }));
    d = await show({ code: 'world_damaged', name: 'X.bhworld' });
    expect(within(d).queryByRole('button', { name: 'Remove from Worlds' })).toBeNull();
    await fireEvent.click(within(d).getAllByRole('button', { name: 'Close' })[1]);
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('x'));
    d = await show({ code: 'world_damaged' });
    await fireEvent.click(within(d).getByRole('button', { name: 'Copy details' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    d = await show({ code: 'declined' });
    expect(within(d).queryByRole('button', { name: 'Start again' })).toBeNull();
    // codes that are not dialogs become toasts; cancelled is silent
    store.shown = null;
    store.fail({ code: 'cancelled' });
    store.fail(new Error('boom'));
    expect(await screen.findByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
  });

  it('dialogs trap focus and give it back', async () => {
    await setup();
    const opener = screen.getByRole('button', { name: 'Add a device' });
    opener.focus();
    store.shown = { error: { code: 'disk_full' }, retry: () => {} };
    const d = await screen.findByRole('dialog');
    await waitFor(() => expect(d.contains(document.activeElement)).toBe(true));
    const buttons = within(d).getAllByRole('button');
    buttons.at(-1)!.focus();
    await fireEvent.keyDown(d, { key: 'Tab' });
    expect(document.activeElement).toBe(buttons[0]);
    await fireEvent.keyDown(d, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(buttons.at(-1));
    buttons[1].focus();
    await fireEvent.keyDown(d, { key: 'Tab' });
    await fireEvent.keyDown(d, { key: 'a' });
    await fireEvent.mouseDown(within(d).getByText(/Free up space/));
    expect(store.shown).not.toBeNull();
    await fireEvent.mouseDown(d.parentElement as HTMLElement);
    await waitFor(() => expect(store.shown).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it('the page boots the mock outside Tauri', async () => {
    resetStore();
    const { default: Page } = await import('../../routes/+page.svelte');
    render(Page);
    await waitFor(() => expect(store.booted).toBe(true));
    const { default: Layout } = await import('../../routes/+layout.svelte');
    const { createRawSnippet } = await import('svelte');
    cleanup();
    render(Layout, { children: createRawSnippet(() => ({ render: () => '<p>child</p>' })) });
    expect(screen.getByText('child')).toBeInTheDocument();
  });
});
