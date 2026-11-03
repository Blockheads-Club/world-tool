import { fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';
import { cleanup } from '@testing-library/svelte';

afterEach(() => cleanup());

describe('Devices', () => {
  it('lists phones, Blockheads PC and iPhone backups with status and actions', async () => {
    await setup();
    expect(screen.getByRole('heading', { name: 'Devices', level: 1 })).toBeInTheDocument();
    const phones = screen.getByRole('region', { name: 'Phones and tablets' });
    expect(within(phones).getAllByText('Ready')).toHaveLength(3);
    expect(within(phones).getByText('Not allowed yet')).toBeInTheDocument();
    expect(within(phones).getByText('Offline')).toBeInTheDocument();
    expect(within(phones).getByText('Last seen 2 days ago')).toBeInTheDocument();
    expect(within(phones).getByText('Root: direct access')).toBeInTheDocument();
    expect(within(phones).getByText('Wi-Fi · 192.168.1.42:37115')).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'On this computer' })).toHaveTextContent('Found automatically · 5 worlds');
    const ios = screen.getByRole('region', { name: 'iPhone and iPad backups' });
    expect(within(ios).getByText('Read-only')).toBeInTheDocument();
    expect(within(ios).getByText('Encrypted')).toBeInTheDocument();
    // sidebar: ready count and connected card
    expect(screen.getByLabelText('5 devices ready')).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Connected' })).toHaveTextContent('Ada’s Pixel 8');
  });

  it('opens how-to-allow, refreshes, reconnects', async () => {
    await setup();
    await fireEvent.click(screen.getByRole('button', { name: 'How to allow' }));
    const dlg = await screen.findByRole('dialog', { name: 'Allow this computer on the phone' });
    expect(dlg).toHaveTextContent('moto g(7) is connected');
    await fireEvent.click(within(dlg).getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Refresh' })[0]);
    await waitFor(() => expect(store.scanning).toBe(false));
  });

  it('starts reading, writing and the guides from a row; the More menu', async () => {
    await setup();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Write worlds' })[0]);
    expect(store.screen).toBe('transfer');
    expect(store.transfer.to).toBe('pixel8');
    store.go('devices');
    await waitFor(() => screen.getByRole('button', { name: 'Export worlds' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Export worlds' }));
    expect(store.read.device?.id).toBe('iphone');
    store.go('devices');
    await waitFor(() => screen.getByRole('button', { name: 'More actions for Galaxy Tab S6 Lite' }));
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Galaxy Tab S6 Lite' }));
    const menu = screen.getByRole('menu');
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    await fireEvent.keyDown(menu, { key: 'ArrowUp' });
    await fireEvent.keyDown(menu, { key: 'x' });
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Copied to the clipboard')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Galaxy Tab S6 Lite' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Forget' }));
    await waitFor(() => expect(store.scanning).toBe(false));
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Ada’s Pixel 8' }));
    await fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Ada’s iPhone' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Connection guide' }));
    expect(store.guide).toBe('ios');
    store.go('devices');
    await waitFor(() => screen.getByRole('button', { name: 'More actions for Pixel 6a' }));
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Pixel 6a' }));
    await fireEvent.mouseDown(document.querySelector('.shield') as Element);
    expect(screen.queryByRole('menu')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'More actions for Ada’s Pixel 8' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Connection guide' }));
    expect(store.guide).toBe('usb');
    store.go('devices');
    await waitFor(() => screen.getByRole('button', { name: 'Add a device' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Add a device' }));
    expect(store.screen).toBe('connect');
  });

  it('shows "looking for devices" and "no device found"', async () => {
    const { mock } = await setup({ scenario: 'empty', before: (m) => (m.settings.first_run_done = true) });
    store.scanning = true;
    expect(await screen.findByText('Looking for devices…')).toBeInTheDocument();
    store.scanning = false;
    expect(await screen.findByText('No device found')).toBeInTheDocument();
    for (const [name, guide] of [['USB cable', 'usb'], ['Wi-Fi pairing', 'wifi'], ['iPhone backup', 'ios'], ['Nothing shows up? Troubleshooting', 'trouble']] as const) {
      store.go('devices');
      await fireEvent.click(await screen.findByRole('button', { name }));
      expect(store.guide).toBe(guide);
    }
    expect(mock).toBeTruthy();
  });

  it('shows Blockheads PC as the connected card when no phone is ready; copy fails quietly', async () => {
    await setup({ before: (m) => (m.devices = m.devices.filter((d) => d.kind !== 'android')) });
    expect(screen.getByRole('group', { name: 'Connected' })).toHaveTextContent('Blockheads PC');
  });
});
