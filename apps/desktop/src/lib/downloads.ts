import { invoke, isTauri } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';

export type DownloadStatus = 'idle' | 'resolving' | 'ready' | 'preparing' | 'downloading' | 'verifying' | 'saving' | 'complete' | 'cancelled' | 'failed';
export interface Release { version: string; file_name: string; length: number; sha256: string; signer_fingerprint: string }
export interface DownloadSnapshot {
  status: DownloadStatus; release: Release | null; received_bytes: number; total_bytes: number;
  image_locked?: boolean;
  existing_image: boolean; cancel_requested: boolean; image_path: string | null; error: string | null; destination_directory: string; host_os: string; host_architecture: string;
}
export const isActive = (status?: DownloadStatus) => !!status && ['resolving', 'preparing', 'downloading', 'verifying', 'saving'].includes(status);
export const formatBytes = (bytes: number) => bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(2)} GiB` : `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
type Invoke = <T>(command: string) => Promise<T>;
export function createDownloads(call: Invoke, native: boolean) {
  const state = writable<{ snapshot: DownloadSnapshot | null; error: string | null; pending: boolean }>({ snapshot: null, error: null, pending: false });
  let epoch = 0;
  let reading = false;
  async function refresh() {
    if (!native || reading || get(state).pending) return;
    reading = true;
    const requestEpoch = epoch;
    try {
      const snapshot = await call<DownloadSnapshot>('download_status');
      if (epoch === requestEpoch) state.update(value => ({ ...value, snapshot, error: null }));
    } catch (error) {
      if (epoch === requestEpoch) state.update(value => ({ ...value, error: String(error) }));
    } finally { reading = false; }
  }
  async function command(name: 'resolve_download' | 'start_download' | 'cancel_download' | 'choose_download_directory') {
    if (!native || get(state).pending) return;
    epoch += 1;
    state.update(value => ({ ...value, pending: true, error: null }));
    try {
      const snapshot = await call<DownloadSnapshot>(name);
      state.update(value => ({ ...value, snapshot }));
    } catch (error) {
      state.update(value => ({ ...value, error: String(error) }));
    } finally { state.update(value => ({ ...value, pending: false })); }
  }
  return { subscribe: state.subscribe, native, refresh, resolve: () => command('resolve_download'), start: () => command('start_download'), cancel: () => command('cancel_download'), chooseDirectory: () => command('choose_download_directory') };
}
export const downloads = createDownloads(invoke, isTauri());
