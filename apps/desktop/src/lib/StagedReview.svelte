<script lang="ts">
  import { TriangleAlert } from 'lucide-svelte';
  import type { StagedDisk, StagedRegion, StagedReviewPlan } from './setup';
  import { afterLayout, barWidths, MiB, reviewBullets, reviewWarning } from './stagedLayout';
  // The reviewed plan before any disk change, in place of the choose screen.
  export let summary:string;
  export let plan:StagedReviewPlan|null|undefined=undefined;
  // The inspected disk the plan is for, to draw Now and After.
  export let disk:StagedDisk|undefined=undefined;
  export let secureBoot:boolean|null|undefined=undefined;
  export let pending=false;
  export let respond:(approved:boolean)=>void=()=>{};
  const CHAR_PX=6.6, PADDING_PX=16, GAP_PX=2;
  // `used` is the share of a partition in use, drawn as on the map.
  type Item={kind:'partition'|'free'|'temporary'|'omarchy';label:string;sizeBytes:number;used:number|null};
  let width=0;
  $: after=plan && disk ? afterLayout(disk,plan) : null;
  // Bars only when the plan matches the inspection; the bullets say the same
  // in words.
  $: rows=after && disk ? [
    {name:'Now',items:disk.regions.filter(r=>r.kind!=='free' || r.sizeBytes>=5*MiB).map(r=>item(r.kind,r.label,r.sizeBytes,r))},
    {name:'After',items:after.map(r=>item(r.kind,r.label,r.sizeBytes,disk?.regions.find(d=>!!r.partitionGuid && d.partitionGuid===r.partitionGuid)))}
  ].map(row=>({...row,widths:barWidths(row.items.map(i=>i.sizeBytes),{barPx:width || 640})})) : [];
  // Replacing names the partition when it isn't the Windows one.
  $: windows=!!plan?.shrink && !!disk?.regions.find(r=>r.partitionGuid===plan?.shrink?.partitionGuid)?.isWindows;
  $: bullets=plan ? reviewBullets(plan,secureBoot,windows) : [];
  $: warning=plan ? reviewWarning(plan) : null;
  // The summary's first paragraph repeats the warning shown above.
  $: details=summary.split('\n\n').slice(warning ? 1 : 0);
  // A shrunk partition keeps its used space, so its share grows After.
  function item(kind:Item['kind'],label:string,sizeBytes:number,inspected?:StagedRegion):Item {
    const usedBytes=inspected?.kind==='partition' && inspected.freeBytes!=null ? inspected.sizeBytes-inspected.freeBytes : null;
    return {kind,label:kind==='free' ? 'Free' : kind==='temporary' ? '' : label,sizeBytes,used:usedBytes!=null && sizeBytes>0 ? Math.min(100,usedBytes/sizeBytes*100) : null};
  }
  // Names only, where they fit; the bullets give the sizes.
  function fitting(text:string,percent:number,px:number,count:number) {
    const drawn=Math.max(0,px-GAP_PX*Math.max(0,count-1));
    return text && text.length*CHAR_PX+PADDING_PX<=percent/100*drawn ? text : '';
  }
</script>

<div class="review" role="region" aria-label="Review changes">
  {#if plan}
    <h3>Prepare Disk {plan.diskNumber}?</h3>
    {#if rows.length}
      <div class="bars" aria-hidden="true" bind:clientWidth={width}>
        {#each rows as row}
          <span class="name">{row.name}</span>
          <div class="bar">
            {#each row.items as segment,index}
              {@const text=fitting(segment.label,row.widths[index],width,row.items.length)}
              <span class="segment {segment.kind}" class:used={segment.used!=null} style:flex-grow={row.widths[index]} style:--used={`${segment.used ?? 0}%`}>{#if text}<span class="text">{text}</span>{/if}</span>
            {/each}
          </div>
        {/each}
        <span></span>
        <div class="legend"><span><i class="swatch temporary"></i>Temporary installer</span>{#if plan.omarchy}<span><i class="swatch omarchy"></i>Omarchy</span>{/if}</div>
      </div>
    {/if}
    <ul>{#each bullets as bullet}<li>{bullet}</li>{/each}</ul>
    {#if warning}<p class="warning"><TriangleAlert size={14} aria-hidden="true"/>{warning}</p>{/if}
    <details><summary>Full details</summary>{#each details as paragraph}<p>{paragraph}</p>{/each}</details>
  {:else}
    <h3>Review changes</h3>
    {#each summary.split('\n\n') as paragraph}<p>{paragraph}</p>{/each}
  {/if}
  <div class="actions"><button disabled={pending} onclick={()=>respond(false)}>Cancel</button><button class="primary" disabled={pending} onclick={()=>respond(true)}>Prepare installer</button></div>
</div>

<style>
  .review{border:1px solid var(--accent);border-radius:6px;background:var(--surface);padding:22px;margin:24px 0 16px}
  h3{font-size:13px;margin:0 0 14px}
  .bars{display:grid;grid-template-columns:auto 1fr;align-items:center;gap:8px 12px;margin:0 0 14px}
  .name{font-size:11px;color:var(--text-dim)}
  .bar{display:flex;gap:2px;height:30px;min-width:0}
  .segment{flex:0 1 0;min-width:0;display:flex;align-items:center;justify-content:center;overflow:hidden;white-space:nowrap;font-size:11px;color:var(--text);background:var(--surface-raised)}
  .used{background:linear-gradient(to right,var(--border-strong) var(--used),var(--surface-raised) var(--used))}
  /* Partitions without a measured fill, such as system ones, as on the map. */
  .partition:not(.used){background:var(--border)}
  .segment:first-child{border-radius:3px 0 0 3px}.segment:last-child{border-radius:0 3px 3px 0}
  .free{background:transparent;box-shadow:none;border:1px dashed var(--border-strong)}
  .temporary{background:repeating-linear-gradient(45deg,color-mix(in srgb,var(--accent-hover) 55%,var(--surface-deep)) 0 3px,var(--surface-deep) 3px 7px)}
  .omarchy{background:var(--accent)}
  .legend{display:flex;flex-wrap:wrap;gap:2px 16px;margin-top:-2px;font-size:11px;color:var(--text-dim)}
  .swatch{display:inline-block;width:14px;height:10px;margin-right:6px;border-radius:2px;vertical-align:-1px}
  .text{margin:0 6px;padding:1px 4px;border-radius:2px;background:rgb(19 20 28/.75);overflow:hidden}
  ul{margin:0 0 12px;padding-left:18px;font-size:12px;line-height:1.8;color:var(--text)}
  p{margin:0 0 8px;font-size:12px;line-height:1.8;color:var(--text-muted);white-space:pre-line}
  .warning{display:flex;align-items:center;gap:8px;color:var(--text)}.warning :global(svg){flex:none;color:var(--accent-hover)}
  details{font-size:11px;color:var(--text-dim);line-height:1.8;margin-bottom:14px}summary{cursor:pointer;padding:4px 0}details p{font-size:11px}
  .actions{display:flex;justify-content:flex-end;gap:10px;border-top:1px solid var(--border);padding-top:18px}
  button{display:inline-flex;align-items:center;justify-content:center;gap:9px;min-height:42px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:hover:not(:disabled){border-color:var(--accent);background:var(--surface-hover)}button:disabled{opacity:.5;cursor:not-allowed}
  .primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  @media(max-width:550px){.review{padding:16px}.actions{flex-direction:column-reverse;align-items:stretch}}
</style>
