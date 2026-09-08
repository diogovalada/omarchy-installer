import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';
import App from '../App.svelte';
import DownloadPanel from './DownloadPanel.svelte';
import { downloads, type DownloadSnapshot } from './downloads';

vi.mock('./downloads', async importOriginal => {
  const actual = await importOriginal<typeof import('./downloads')>();
  const { writable } = await import('svelte/store');
  const state = writable({ snapshot: null, pending: false, error: null });
  return { ...actual, downloads: { ...state, native: true, refresh: vi.fn().mockResolvedValue(undefined), resolve: vi.fn(), start: vi.fn(), cancel: vi.fn(), chooseDirectory: vi.fn() } };
});

const controller = downloads as typeof downloads & { set: (value: { snapshot: DownloadSnapshot; pending: boolean; error: string | null }) => void };
const state = (status: DownloadSnapshot['status'], overrides: Partial<DownloadSnapshot> = {}): DownloadSnapshot => ({
  status, existing_image: false, cancel_requested: false, release: { version: '3.4.1', file_name: 'omarchy-3.4.1.iso', length: 1000, sha256: 'ab'.repeat(32), signer_fingerprint: 'AB'.repeat(20) },
  received_bytes: 1000, total_bytes: 1000, image_path: null, error: null, destination_directory: 'C:\\Users\\Example\\Downloads', host_os: 'windows', host_architecture: 'x86_64', ...overrides
});
const setState = async (snapshot: DownloadSnapshot) => { controller.set({ snapshot, pending: false, error: null }); await tick(); };

describe('single-screen native download flow', () => {
  beforeEach(async () => { vi.clearAllMocks(); await setState(state('ready')); });
  it('locks image changes during installation while allowing details to expand', async () => {
    await setState(state('complete', {image_path:'C:\\fixture.iso'}));
    render(DownloadPanel, {compact:true,locked:true});
    expect(screen.queryByRole('button',{name:'Change'})).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button',{name:'Image details'}));
    expect(screen.getByRole('button',{name:'Change'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Check for updates',hidden:true})).toBeDisabled();
    await fireEvent.click(screen.getByRole('button',{name:'Hide image details'}));
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
  it('downloads explicitly and opens the native destination chooser', async () => {
    render(App);
    await fireEvent.click(screen.getByRole('button', { name: 'Change' }));
    expect(downloads.chooseDirectory).toHaveBeenCalledOnce();
    await fireEvent.click(screen.getByRole('button', { name: 'Download' }));
    expect(downloads.start).toHaveBeenCalledOnce();
    expect(screen.getByText('Details').closest('details')).not.toHaveAttribute('open');
  });
  it('supports pause/resume and does not mark 100% as verified', async () => {
    await setState(state('downloading'));
    render(App);
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Change' })).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    expect(downloads.cancel).toHaveBeenCalledOnce();
    await setState(state('cancelled', { error: 'Download cancelled' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(downloads.start).toHaveBeenCalledOnce();
    await setState(state('verifying'));
    expect(screen.getByRole('status')).toHaveTextContent('Verifying');
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '100');
  });
  it('offers Verify for a detected ISO and updates when changing folders', async () => {
    await setState(state('ready', { existing_image: true }));
    render(App);
    expect(screen.getByRole('status')).toHaveTextContent('Image found');
    await fireEvent.click(screen.getByRole('button', { name: 'Verify' }));
    expect(downloads.start).toHaveBeenCalledOnce();
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    await setState(state('ready', { existing_image: false, destination_directory: 'D:\\Images' }));
    expect(screen.getByRole('button', { name: 'Download' })).toBeEnabled();
    await setState(state('ready', { existing_image: true }));
    expect(screen.getByRole('button', { name: 'Verify' })).toBeEnabled();
  });
  it('requires native completion and a saved path before disk controls appear', async () => {
    await setState(state('complete'));
    render(App);
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    expect(screen.queryByText('Your image is ready. Choose how to install Omarchy.')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Install without USB' })).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', { name: 'Create bootable USB' }));
    expect(screen.getByText('Download and verify the Omarchy image above to continue.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Refresh USB drives' })).not.toBeInTheDocument();
    await setState(state('complete', { image_path: 'C:\\Users\\Example\\Downloads\\omarchy-3.4.1.iso' }));
    expect(screen.getByRole('status')).toHaveTextContent('Verified');
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Image details' }));
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '100');
    expect(screen.queryByRole('button', { name: 'Download' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refresh USB drives' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: /administrator access/i })).not.toBeInTheDocument();
    await setState(state('failed', { error: 'Signature verification failed' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Signature verification failed');
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Refresh USB drives' })).not.toBeInTheDocument();
  });
});
