<script lang="ts">
  import { ArrowRight } from 'lucide-svelte';
  export let label: string;
  export let secondary = false;
  export let disabled = false;
  export let onClick: () => void = () => {};
</script>

<button class:secondary {disabled} onclick={onClick}>
  {label}
  {#if !secondary}<ArrowRight size={15} />{/if}
</button>

<style>
  button { display: inline-flex; align-items: center; justify-content: center; gap: 10px; min-height: 42px; padding: 11px 16px; border: 1px solid var(--accent); border-radius: 4px; background: var(--accent); color: var(--surface-deep); font: inherit; font-size: 11px; font-weight: 700; line-height:1.5; text-transform:uppercase; cursor: pointer; }
  button:hover:not(:disabled) { background: var(--accent-hover); border-color:var(--accent-hover); }
  button:active:not(:disabled) { transform: translateY(0); }
  button.secondary { border-color: var(--border-strong); background: var(--surface-raised); color: var(--text); box-shadow: none; }
  button.secondary:hover:not(:disabled) { background: var(--surface-hover); }
  button:disabled { cursor: not-allowed; opacity: .42; }
</style>
