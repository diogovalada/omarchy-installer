import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import { createDownloads, type DownloadSnapshot } from './downloads';

const snapshot = (status: DownloadSnapshot['status']): DownloadSnapshot => ({ status, existing_image: false, cancel_requested: false, release: null, received_bytes: 0, total_bytes: 0, image_path: null, error: null, destination_directory: 'Downloads/Omarchy', host_os: 'windows', host_architecture: 'x86_64' });
describe('native download controller', () => {
  it('does not simulate native actions in a browser', async () => {
    const call = vi.fn();
    const controller = createDownloads(call, false);
    await controller.refresh(); await controller.resolve(); await controller.start();
    expect(call).not.toHaveBeenCalled();
    expect(get(controller).snapshot).toBeNull();
  });
  it('reports native failures and allows a retry', async () => {
    const call = vi.fn().mockRejectedValueOnce('network failed').mockResolvedValue(snapshot('ready'));
    const controller = createDownloads(call, true);
    await controller.resolve();
    expect(get(controller)).toMatchObject({ error: 'network failed', pending: false });
    await controller.resolve();
    expect(get(controller)).toMatchObject({ error: null, snapshot: { status: 'ready' } });
    expect(call.mock.calls).toEqual([['resolve_download'], ['resolve_download']]);
  });
  it('ignores stale polling results and does not duplicate active commands', async () => {
    let finishPoll!: (value: DownloadSnapshot) => void;
    let finishCommand!: (value: DownloadSnapshot) => void;
    const call = vi.fn().mockImplementationOnce(() => new Promise(resolve => { finishPoll = resolve; })).mockImplementationOnce(() => new Promise(resolve => { finishCommand = resolve; }));
    const controller = createDownloads(call, true);
    const poll = controller.refresh();
    const start = controller.start();
    await controller.start();
    finishCommand(snapshot('downloading')); await start;
    finishPoll(snapshot('ready')); await poll;
    expect(get(controller).snapshot?.status).toBe('downloading');
    expect(call).toHaveBeenCalledTimes(2);
  });
  it('takes cancellation and completion from native state', async () => {
    const call = vi.fn().mockResolvedValueOnce(snapshot('downloading')).mockResolvedValueOnce(snapshot('cancelled')).mockResolvedValueOnce(snapshot('complete'));
    const controller = createDownloads(call, true);
    await controller.start(); await controller.cancel();
    expect(get(controller).snapshot?.status).toBe('cancelled');
    await controller.refresh();
    expect(get(controller).snapshot?.status).toBe('complete');
    expect(call.mock.calls).toEqual([['start_download'], ['cancel_download'], ['download_status']]);
  });
});
