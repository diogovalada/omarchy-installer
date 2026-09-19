<script lang="ts">
  import { LockKeyhole } from 'lucide-svelte';
</script>
<aside aria-label="BitLocker preparation">
  <LockKeyhole size={16}/>
  <p>BitLocker stays on while you choose space. We’ll ask to suspend protection when you prepare the installer, without decrypting your files.</p>
</aside>
<style>
  aside{display:flex;align-items:flex-start;gap:10px;color:var(--accent);margin:16px 0}
  p{margin:0;font-size:11px;line-height:1.8;color:var(--text-muted)}
  aside :global(svg){flex-shrink:0;margin-top:3px}
</style>
