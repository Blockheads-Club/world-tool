import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import { store } from '../store.svelte';
import { setup } from '../test-utils';

afterEach(() => cleanup());

const guide = async (name: RegExp) => fireEvent.click(within(screen.getByRole('navigation', { name: 'Connection types' })).getByRole('button', { name }));

describe('Add a device (guides)', () => {
  it('USB: steps, waiting, a phone that arrives; Windows driver links', async () => {
    const { mock } = await setup();
    store.info.os = 'windows';
    store.openGuide('usb');
    await screen.findByRole('heading', { name: 'USB cable', level: 2 });
    expect(screen.getByText('Waiting for a device…')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Phone screen: Allow USB debugging?' })).toBeInTheDocument();
    mock.plug({ ...store.devices[0], id: 'new', name: 'Pixel 9' });
    expect(await screen.findByText('Pixel 9 is connected')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Read worlds' }));
    expect(store.read.device?.name).toBe('Pixel 9');
    store.openGuide('usb');
    await screen.findByText('Waiting for a device…');
    mock.plug({ ...store.devices[0], id: 'new2', name: 'Pixel 10', game_version: null });
    await screen.findByText('Pixel 10 is connected');
    await fireEvent.click(await screen.findByRole('button', { name: 'Install the USB driver' }));
    expect(screen.getByRole('heading', { name: 'Windows USB driver', level: 2 })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open Device Manager' }));
    await guide(/USB cable/);
    await fireEvent.click(screen.getAllByRole('button', { name: 'Troubleshooting' }).at(-1) as HTMLElement);
    expect(store.guide).toBe('trouble');
    store.info.os = 'linux';
    await guide(/USB cable/);
    await fireEvent.click(screen.getByRole('button', { name: 'Nothing shows up? Troubleshooting' }));
    expect(store.guide).toBe('trouble');
    await fireEvent.click(screen.getByRole('button', { name: 'Back to devices' }));
    expect(store.screen).toBe('devices');
  });

  it('Wi-Fi: found on the network, pair, wrong code', async () => {
    const { mock } = await setup();
    store.openGuide('wifi');
    await screen.findByText('Galaxy Tab S6 Lite (192.168.1.42) is ready to pair.');
    const pair = screen.getByRole('button', { name: 'Pair' });
    expect(pair).toBeDisabled();
    await fireEvent.input(screen.getByLabelText('IP address and port'), { target: { value: '10.0.0.2:1' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    expect(screen.getByLabelText('IP address and port')).toHaveValue('192.168.1.42:37115');
    await fireEvent.input(screen.getByLabelText('Pairing code'), { target: { value: '12' } });
    await fireEvent.click(pair);
    expect(await screen.findByRole('alert')).toHaveTextContent('Pairing failed');
    await fireEvent.input(screen.getByLabelText('Pairing code'), { target: { value: '482 915' } });
    await fireEvent.click(pair);
    expect(await screen.findByText('Galaxy Tab S6 Lite is paired')).toBeInTheDocument();
    expect(screen.getByLabelText('Pairing code')).toHaveValue('');
    expect(mock).toBeTruthy();
  });

  it('Root: choose the mode; Blockheads PC: found, folder, not found', async () => {
    const { mock } = await setup();
    store.openGuide('root');
    await screen.findByText('Root gives full access to the phone');
    await fireEvent.click(screen.getByRole('radio', { name: 'Backup method' }));
    await waitFor(() => expect(store.settings?.root_mode).toBe('never'));
    await guide(/Blockheads PC/);
    expect(screen.getByText('Found Blockheads PC on this computer')).toBeInTheDocument();
    expect(screen.getByText(/5 worlds · The Blockheads 1\.7\.6/)).toBeInTheDocument();
    expect(screen.getByLabelText('Game data folder')).toHaveValue('/home/ada/.local/share/Blockheads PC');
    mock.picks.folder = null;
    await fireEvent.click(screen.getByRole('button', { name: 'Change…' }));
    mock.picks.folder = '/portable/BH';
    await fireEvent.click(screen.getByRole('button', { name: 'Change…' }));
    await waitFor(() => expect(screen.getByLabelText('Game data folder')).toHaveValue('/portable/BH'));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Back to devices' })[1]);
    store.openGuide('pc');
    await fireEvent.click(await screen.findByRole('button', { name: 'Read worlds' }));
    expect(store.read.device?.id).toBe('pc');
    store.devices = store.devices.filter((d) => d.kind !== 'pc');
    store.settings!.pc_folder = null;
    store.openGuide('pc');
    expect(await screen.findByText('Blockheads PC was not found')).toBeInTheDocument();
    expect(screen.getByLabelText('Game data folder')).toHaveValue('');
    await fireEvent.click(screen.getByRole('button', { name: 'Change…' }));
  });

  it('iPhone backups: choose, password, open; browse; none found', async () => {
    const { mock } = await setup();
    store.openGuide('ios');
    expect(await screen.findByLabelText('Backup password for Ada’s iPhone')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open backup' }));
    expect(await screen.findByText('The password doesn’t match.')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: 'Family iPad' }));
    expect(screen.queryByLabelText(/Backup password/)).toBeNull();
    expect(screen.getByText('Not encrypted')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('radio', { name: 'Ada’s iPhone' }));
    await fireEvent.input(screen.getByLabelText('Backup password for Ada’s iPhone'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Open backup' }));
    await waitFor(() => expect(store.screen).toBe('read'));
    expect(store.read.device?.id).toBe('iphone');
    store.openGuide('ios');
    mock.picks.folder = null;
    await fireEvent.click(await screen.findByRole('button', { name: 'Browse…' }));
    mock.ios = [];
    mock.picks.folder = '/copied';
    await fireEvent.click(screen.getByRole('button', { name: 'Browse…' }));
    expect(await screen.findByText(/No backups found here/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open backup' })).toBeNull();
  });

  it('Troubleshooting: restart the service, copy diagnostics', async () => {
    await setup();
    store.openGuide('trouble');
    await screen.findByText('“Allow USB debugging?” never appears');
    await fireEvent.click(screen.getByRole('button', { name: 'Restart connection service' }));
    expect(await screen.findByText('Connection service restarted')).toBeInTheDocument();
    const { copyText } = await import('../util');
    expect(copyText).toBeTypeOf('function');
    const { vi } = await import('vitest');
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('no'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostic info' }));
    expect(await screen.findByText('Copied to the clipboard')).toBeInTheDocument();
  });
});
