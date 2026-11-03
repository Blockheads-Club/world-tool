import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { setup } from '../test-utils';

afterEach(() => cleanup());

describe('Activity', () => {
  it('shows history with filters, error details, copy and clear', async () => {
    const { mock } = await setup();
    mock.activity.push(
      { id: 'x1', time: Date.now() - 3 * 86_400_000, kind: 'restore', result: 'done', device: 'Ada’s Pixel 8', worlds: [], size: null, detail: null, error: null },
      { id: 'x2', time: Date.now() - 4 * 86_400_000, kind: 'export', result: 'failed', device: 'Worlds', worlds: [], size: null, detail: null, error: null }
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
    const table = await screen.findByRole('table', { name: 'Activity' });
    expect(within(table).getByText('Copied 2 worlds to Ada’s Pixel 8')).toBeInTheDocument();
    expect(within(table).getByText('Copy to Galaxy Tab S6 Lite stopped')).toBeInTheDocument();
    expect(within(table).getByText('Cable or Wi-Fi lost. Nothing was written.')).toBeInTheDocument();
    expect(within(table).getByText('Read from moto g(7) failed')).toBeInTheDocument();
    expect(within(table).getAllByText('Error details')).toHaveLength(2);
    expect(within(table).getByText('Stopped')).toBeInTheDocument();
    expect(within(table).getByText('Backup restored to Ada’s Pixel 8')).toBeInTheDocument();
    expect(within(table).getByText('Export failed')).toBeInTheDocument();
    expect(within(table).getByText('Something went wrong. Details are in Activity.')).toBeInTheDocument();
    const chips = screen.getByRole('group', { name: 'Show' });
    for (const [name, rows] of [['Reads', 3], ['Copies', 3], ['Backups', 2], ['Errors', 3], ['All', 8]] as const) {
      await fireEvent.click(within(chips).getByRole('button', { name }));
      expect(screen.getAllByRole('row').length - 1).toBe(rows);
    }
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith(expect.stringContaining('E_DEVICE_GONE'));
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('x'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy log' }));
    expect(await screen.findByText('Could not copy to the clipboard')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Cancel' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    await fireEvent.click(within(await screen.findByRole('alertdialog', { name: 'Clear the activity log?' })).getByRole('button', { name: 'Clear' }));
    expect(await screen.findByText('No activity yet')).toBeInTheDocument();
  });

  it('says when a filter has nothing', async () => {
    const { mock } = await setup();
    mock.activity = mock.activity.filter((a) => a.result === 'done');
    await fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
    await screen.findByRole('table');
    await fireEvent.click(screen.getByRole('button', { name: 'Errors' }));
    await waitFor(() => expect(screen.getByText('Nothing of this kind yet.')).toBeInTheDocument());
  });
});
