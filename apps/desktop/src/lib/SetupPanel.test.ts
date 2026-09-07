import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';
import SetupPanel from './SetupPanel.svelte';
import { setup, type SetupSnapshot } from './setup';
import { downloads } from './downloads';

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

const setupState = setup as typeof setup & {set:(state:unknown)=>void};
const downloadState = downloads as typeof downloads & {set:(state:unknown)=>void};
const ready = ():SetupSnapshot => ({kind:'direct',status:'ready',stage:'ready',message:'Ready.',choices:[],requirements:[],bytes:0,totalBytes:0,cancelAvailable:false,cancelRequested:false,error:null,receipt:null,preparation:{secureBoot:'enabled',runtimePackaged:true,runtimeReady:false}});
describe('separate Windows preparation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupState.set({pending:false,error:null,snapshot:ready()});
    downloadState.set({snapshot:{status:'complete',image_path:'C:\\fixture.iso'}});
  });
  it('requires the saved-work and recovery acknowledgement before requesting a firmware restart', async () => {
    render(SetupPanel, { kind: 'direct', close: vi.fn() });
    const restart = screen.getByRole('button', { name: 'Restart to firmware settings' });
    expect(restart).toBeDisabled();
    expect(calls.prepare).not.toHaveBeenCalled();
    expect(screen.getByText('Prepare this computer, then refresh disks to choose space for Omarchy.')).toBeInTheDocument();
    expect(screen.queryByText('Installation requirements and encryption')).not.toBeInTheDocument();
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
    await fireEvent.click(screen.getByRole('button', { name: 'Load installation tools' }));
    expect(calls.prepare).toHaveBeenCalledExactlyOnceWith('runtime');
    expect(calls.start).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Restart to firmware settings' })).toBeDisabled();
  });
  it('keeps progress and verified completion visible when the image state changes', async () => {
    setupState.set({pending:false,error:null,snapshot:{...ready(),status:'running',message:'Writing Omarchy system…'}});
    render(SetupPanel, {kind:'direct',close:vi.fn()});
    downloadState.set({snapshot:{status:'failed',image_path:null}});
    await tick();
    expect(screen.getByRole('status')).toHaveTextContent('Writing Omarchy system…');
    expect(screen.getByRole('progressbar')).toBeInTheDocument();
    expect(screen.queryByText('Turn off Secure Boot')).not.toBeInTheDocument();
    expect(screen.queryByText('Installation requirements and encryption')).not.toBeInTheDocument();
    setupState.set({pending:false,error:null,snapshot:{...ready(),status:'complete',receipt:{receipt:{bitLockerRestoration:{required:true,verified:true}}}}});
    await tick();
    expect(screen.getByRole('status')).toHaveTextContent('Omarchy has been installed.');
    expect(screen.getByText('Windows BitLocker protection has been restored and verified.')).toBeInTheDocument();
  });
});
