<script lang="ts">
  import { Monitor, Laptop, Command, Terminal } from 'lucide-svelte';
  import { hosts, type HostId } from './model';
  export let selected: HostId;
  export let onSelect: (host: HostId) => void;

  const iconFor = { windows: Monitor, 'intel-mac': Laptop, 'apple-silicon': Command, linux: Terminal };
</script>

<div class="platform-wrap">
  <div>
    <p class="eyebrow">Simulation host</p>
    <p class="helper">Preview capabilities without inspecting this computer</p>
  </div>
  <div class="platforms" role="radiogroup" aria-label="Simulated platform">
    {#each hosts as host}
      <button class:active={selected === host.id} role="radio" aria-checked={selected === host.id} onclick={() => onSelect(host.id)}>
        <svelte:component this={iconFor[host.id]} size={15} strokeWidth={1.8} />
        <span>{host.shortLabel}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .platform-wrap { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 14px 18px; border: 1px solid var(--border); border-radius: 14px; background: rgba(255,255,255,.025); }
  .eyebrow { margin: 0 0 2px; color: var(--text); font-size: 12px; font-weight: 700; letter-spacing: .01em; }
  .helper { margin: 0; color: var(--text-dim); font-size: 11px; }
  .platforms { display: flex; gap: 4px; padding: 4px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-deep); }
  button { display: inline-flex; align-items: center; gap: 7px; min-height: 32px; padding: 0 10px; border: 1px solid transparent; border-radius: 7px; background: transparent; color: var(--text-dim); font: inherit; font-size: 11px; font-weight: 630; cursor: pointer; }
  button:hover { color: var(--text); background: rgba(255,255,255,.04); }
  button.active { color: var(--text); background: var(--surface-raised); border-color: var(--border-strong); box-shadow: 0 1px 4px rgba(0,0,0,.26); }
  @media (max-width: 900px) { .platform-wrap { align-items: stretch; flex-direction: column; gap: 10px; } .platforms { overflow-x: auto; } button { flex: 1 0 auto; justify-content: center; } }
</style>
