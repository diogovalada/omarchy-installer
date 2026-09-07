import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';
import SetupPanel from './SetupPanel.svelte';

const calls = vi.hoisted(() => ({ prepare: vi.fn(), inspect: vi.fn(), start: vi.fn() }));
vi.mock('./downloads', async importOriginal => {
  const actual = await importOriginal<typeof import('./downloads')>();
  const { writable } = await import('svelte/store');
  return { ...actual, downloads: { ...writable({ snapshot: { status: 'complete', image_path: 'C:\\fixture.iso' } }), native: true } };
});
vi.mock('./setup', async importOriginal => {
  const actual = await importOriginal<typeof import('./setup')>();
  const { writable } = await import('svelte/store');
  const state = writable({ pending: false, error: null, snapshot: { kind: 'direct', status: 'ready', choices: [], requirements: [], preparation: { secureBoot: 'enabled', runtimePackaged: true, runtimeReady: false } } });
  return { ...actual, setup: { ...state, ...calls } };
});

describe('separate Windows preparation', () => {
  beforeEach(() => { vi.clearAllMocks(); });
  it('requires the saved-work and recovery acknowledgement before requesting a firmware restart', async () => {
    render(SetupPanel, { kind: 'direct', close: vi.fn() });
    const restart = screen.getByRole('button', { name: 'Prepare and restart to firmware' });
    expect(restart).toBeDisabled();
    expect(calls.prepare).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('checkbox'));
    expect(restart).toBeEnabled();
    await fireEvent.click(restart);
    await tick();
    expect(calls.prepare).toHaveBeenCalledExactlyOnceWith('firmware');
    expect(calls.start).not.toHaveBeenCalled();
    expect(restart).toBeDisabled();
    expect(screen.getByText(/leave TPM enabled/)).toBeInTheDocument();
  });
  it('imports the packaged runtime without starting installation or restarting firmware', async () => {
    render(SetupPanel, { kind: 'direct', close: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Import packaged runtime' }));
    expect(calls.prepare).toHaveBeenCalledExactlyOnceWith('runtime');
    expect(calls.start).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Prepare and restart to firmware' })).toBeDisabled();
  });
});
