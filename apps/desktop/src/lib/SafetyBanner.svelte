<script lang="ts">
  import { ShieldCheck, FlaskConical } from 'lucide-svelte';
  export let compact = false;
</script>

<div class:compact class="banner" role="status">
  <div class="icon"><FlaskConical size={17} /></div>
  <div class="copy">
    <strong>Safe simulation</strong>
    <span>This preview cannot access disks, request administrator access, or change this computer.</span>
  </div>
  {#if !compact}<ShieldCheck class="shield" size={18} />{/if}
</div>

<style>
  .banner { display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid color-mix(in srgb, var(--accent) 22%, var(--border)); border-radius: 12px; background: color-mix(in srgb, var(--accent) 5%, var(--surface)); }
  .icon { display: grid; flex: 0 0 32px; height: 32px; place-items: center; border-radius: 9px; background: color-mix(in srgb, var(--accent) 13%, transparent); color: var(--accent); }
  .copy { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 2px; }
  strong { color: var(--text); font-size: 12px; }
  span { color: var(--text-muted); font-size: 11px; line-height: 1.4; }
  .shield { color: var(--success); }
  .compact { padding: 10px; }
</style>
