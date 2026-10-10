import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }));

describe('setup error recovery', () => {
  beforeEach(() => { vi.resetModules(); invoke.mockReset(); });
  it('retains an action failure while status polling succeeds, and clears it on a new action', async () => {
    const { setup } = await import('./setup');
    invoke.mockRejectedValueOnce('USB changed').mockResolvedValue({kind:'usb',status:'ready'});
    await setup.reviewUsb('usb','erase');
    await setup.refresh();
    expect(get(setup).error).toBe('USB changed');
    await setup.inspect('usb');
    expect(get(setup).error).toBeNull();
  });
  it('remembers the staged action of the last command only', async () => {
    const { setup } = await import('./setup');
    invoke.mockResolvedValue({kind:null,status:'running'});
    await setup.stagedIso('cleanup',null,'id');
    expect(get(setup).stagedAction).toBe('cleanup');
    // Any other command, such as the USB route, clears it.
    await setup.inspect('usb');
    expect(get(setup).stagedAction).toBeNull();
  });
  it("keeps the failed operation's action when the next command is rejected before starting", async () => {
    const { setup } = await import('./setup');
    invoke.mockResolvedValueOnce({kind:null,status:'running'}).mockResolvedValueOnce({kind:null,status:'failed',error:'Boot entry rejected'});
    await setup.stagedIso('arm',null,'id');
    await setup.refresh();
    invoke.mockRejectedValueOnce('A setup operation is already running').mockResolvedValue({kind:null,status:'failed',error:'Boot entry rejected'});
    await setup.stagedIso('cleanup',null,'id');
    await setup.refresh();
    // The failed snapshot is still the arming's, not a removal's.
    expect(get(setup).stagedAction).toBe('arm');
  });
  it('names the target of a running staged command until another command starts', async () => {
    const { setup } = await import('./setup');
    invoke.mockResolvedValue({kind:null,status:'running'});
    await setup.stagedIso('cleanup',null,'id');
    expect(get(setup).stagedTarget).toEqual({action:'cleanup',operationId:'id'});
    // Answering Review or stopping belongs to the same staged command.
    await setup.respondStagedReview(true);
    await setup.cancel();
    expect(get(setup).stagedTarget).toEqual({action:'cleanup',operationId:'id'});
    // A rejected command leaves the running one's target.
    invoke.mockRejectedValueOnce('A setup operation is already running');
    await setup.stagedIso('inspect');
    expect(get(setup).stagedTarget).toEqual({action:'cleanup',operationId:'id'});
    await setup.inspect('usb');
    expect(get(setup).stagedTarget).toBeNull();
  });
  it('clears a polling-only connection error after reconnecting', async () => {
    const { setup } = await import('./setup');
    invoke.mockRejectedValueOnce('Connection unavailable').mockResolvedValue({kind:'usb',status:'ready'});
    await setup.refresh();
    expect(get(setup).error).toBe('Connection unavailable');
    await setup.refresh();
    expect(get(setup).error).toBeNull();
  });
});
