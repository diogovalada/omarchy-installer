<script lang="ts">
  import { ChevronDown, ChevronRight } from 'lucide-svelte';
  import { formatSize, MiB, regionKey } from './stagedLayout';
  import type { StagedRegion } from './setup';
  // The exact-numbers view and the text alternative to the disk map.
  export let regions:StagedRegion[]=[];
  export let statusText:(region:StagedRegion)=>string=()=>'';
  export let unallocatedBytes=0;
  let open=false;
  const id=`partitions-${Math.random().toString(36).slice(2)}`;
  $: rows=regions.filter(r=>r.kind!=='free' || r.sizeBytes>=5*MiB);
  $: count=regions.filter(r=>r.kind==='partition').length;
</script>

<button type="button" class="toggle" aria-expanded={open} aria-controls={id} onclick={()=>{open=!open;}}>{#if open}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}All partitions ({count})</button>
<div class="table" {id} hidden={!open}>
  <table>
    <thead><tr><th scope="col">Name</th><th scope="col">File system</th><th scope="col">Size</th><th scope="col">Free inside</th><th scope="col">Status</th></tr></thead>
    <tbody>
      {#each rows as region (regionKey(region))}
        <tr><th scope="row">{region.kind==='free' ? 'Free space' : region.label}</th><td>{region.fileSystem ?? ''}</td><td>{formatSize(region.sizeBytes)}</td><td>{region.freeBytes!=null ? formatSize(region.freeBytes,'down') : ''}</td><td>{statusText(region)}</td></tr>
      {/each}
    </tbody>
    <tfoot><tr><td colspan="5">Free space outside partitions: {formatSize(unallocatedBytes)}</td></tr></tfoot>
  </table>
</div>

<style>
  .toggle{grid-area:toggle;justify-self:start;display:inline-flex;align-items:center;gap:6px;min-height:32px;padding:6px 0;border:0;background:transparent;color:var(--text-dim);font:inherit;font-size:11px;cursor:pointer}
  .toggle:hover{color:var(--accent)}
  .table{grid-area:table;margin-top:6px;overflow-x:auto}
  .table[hidden]{display:none}
  table{width:100%;border-collapse:collapse;font-size:11px;color:var(--text-muted)}
  th,td{padding:7px 10px 7px 0;text-align:left;vertical-align:top;border-bottom:1px solid var(--border);font-weight:400;overflow-wrap:anywhere}
  thead th{color:var(--text-dim);font-size:10px}
  tbody th{color:var(--text)}
  tfoot td{border-bottom:0;color:var(--text-dim);font-size:10px}
  td:nth-child(3),td:nth-child(4){white-space:nowrap}
</style>
