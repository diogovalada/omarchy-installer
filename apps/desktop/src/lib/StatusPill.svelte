<script lang="ts">
  import type { Availability } from './model';
  import { statusCopy } from './model';
  export let status: Availability;
</script>

<span class="status" class:ready={status === 'ready'} class:preview={status === 'preview'} class:unavailable={status === 'unavailable'}>
  <span class="dot"></span>{statusCopy(status)}
</span>

<style>
  .status { display: inline-flex; align-items: center; gap: 6px; width: fit-content; color: var(--text-muted); font-size: 12px; font-weight: 650; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--text-dim); }
  .ready { color: var(--success); }
  .ready .dot { background: var(--success); box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 12%, transparent); }
  .preview { color: var(--accent); }
  .preview .dot { background: var(--accent); }
  .unavailable { color: var(--text-dim); }
</style>
