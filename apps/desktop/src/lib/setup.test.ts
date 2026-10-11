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
  it('remembers the last disk check\'s Secure Boot state across other snapshots', async () => {
    const { setup } = await import('./setup');
    invoke.mockResolvedValueOnce({kind:null,status:'complete',stagedIso:{choices:[],secureBoot:true}});
    await setup.stagedIso('inspect');
    expect(get(setup).secureBoot).toBe(true);
    // A later result without the field (preparing, removing) keeps it.
    invoke.mockResolvedValueOnce({kind:null,status:'complete',stagedIso:{operationId:'id',status:'staged'}});
    await setup.stagedIso('cleanup',null,'id');
    expect(get(setup).secureBoot).toBe(true);
    invoke.mockResolvedValueOnce({kind:null,status:'complete',stagedIso:{choices:[],secureBoot:false}});
    await setup.refresh();
    expect(get(setup).secureBoot).toBe(false);
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
