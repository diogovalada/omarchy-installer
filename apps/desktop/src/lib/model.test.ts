import { describe, expect, it } from 'vitest';
import { canContinue, getHost, hosts } from './model';

describe('host capability model', () => {
  it('defines every simulated platform', () => {
    expect(hosts.map((host) => host.id)).toEqual(['windows', 'intel-mac', 'apple-silicon', 'linux']);
  });

  it('fails closed for unavailable capabilities', () => {
    expect(canContinue('unavailable')).toBe(false);
    expect(canContinue('preview')).toBe(true);
  });

  it('falls back safely for an unknown host', () => {
    expect(getHost('unknown' as never).id).toBe('windows');
  });
});
