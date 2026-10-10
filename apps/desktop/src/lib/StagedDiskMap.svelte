<script lang="ts">
  import { tick } from 'svelte';
  import { formatSize, type OverviewSegment, type RegionStatus } from './stagedLayout';
  export let label:string;
  export let segments:OverviewSegment[]=[];
  export let statuses:Record<string,RegionStatus>={};
  export let selected='';
  // The region key of the space source, marked under its segment.
  export let source='';
  export let select:(key:string)=>void=()=>{};
  export let width=0;
  // Off while a reviewed plan or a disk change is in progress.
  export let disabled=false;
  // Label margin and backing padding sit inside the segment, so segment
  // widths keep the scale exactly.
  const CHAR_PX=6.6, PADDING_PX=20, GAP_PX=2;
  let buttons:HTMLButtonElement[]=[];
  $: drawnPx=Math.max(0,width-GAP_PX*Math.max(0,segments.length-1));
  // The longest label that fits inside the segment; small segments show
  // theirs in the selected-partition line instead.
  function fitting(segment:OverviewSegment,px:number) {
    const region=segment.regions[0], size=formatSize(segment.sizeBytes,region.kind==='free' ? 'down' : 'nearest');
    // Windows is named once: "C: · Windows · 953 GiB", but "Windows (C:) · 475 GiB".
    const windows=region.isWindows && !/windows/i.test(region.label) ? ' · Windows' : '';
    const labels=segment.regions.length>1 ? [segment.label] : region.kind==='free' ? [`Free · ${size}`,'Free'] : [`${region.label}${windows} · ${size}`,region.label];
    return labels.find(text=>text.length*CHAR_PX+PADDING_PX<=segment.widthPercent/100*px) ?? '';
  }
  function usedPercent(segment:OverviewSegment) {
    const region=segment.regions[0];
    if(segment.regions.length>1 || region.kind!=='partition' || region.freeBytes==null || region.sizeBytes<=0) return null;
    return Math.min(100,Math.max(0,(region.sizeBytes-region.freeBytes)/region.sizeBytes*100));
  }
  async function keydown(event:KeyboardEvent,index:number) {
    const last=segments.length-1;
    const next={ArrowRight:index+1,ArrowDown:index+1,ArrowLeft:index-1,ArrowUp:index-1,Home:0,End:last}[event.key];
    if(next===undefined) return;
    event.preventDefault();
    const target=next>last ? 0 : next<0 ? last : next;
    select(segments[target].key);
    await tick();
    buttons[target]?.focus();
  }
</script>

<div class="map" role="radiogroup" aria-label={label} bind:clientWidth={width}>
  {#each segments as segment,index (segment.key)}
    {@const status=statuses[segment.key]}
    {@const used=usedPercent(segment)}
    {@const text=fitting(segment,drawnPx)}
    <button type="button" role="radio" class="segment {status?.tone ?? 'normal'}" class:selected={segment.key===selected} class:source={segment.regions.length===1 && segment.key===source} class:used={used!=null && ['normal','dimmed'].includes(status?.tone ?? 'normal')} style:flex-grow={segment.widthPercent} style:--used={`${used ?? 0}%`} aria-checked={segment.key===selected} aria-label={status?.text || segment.label} tabindex={segment.key===selected ? 0 : -1} {disabled} bind:this={buttons[index]} onclick={()=>select(segment.key)} onkeydown={event=>keydown(event,index)}>
      {#if text}<span class="text" aria-hidden="true">{text}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .map{display:flex;gap:2px;height:40px;margin:14px 0 10px}
  .segment{position:relative;flex:0 1 0;min-width:0;display:flex;align-items:center;padding:0;border:0;border-radius:0;background:var(--surface-raised);color:var(--text);font:inherit;font-size:11px;white-space:nowrap;overflow:hidden;cursor:pointer;appearance:none}
  .segment:first-child{border-radius:3px 0 0 3px}.segment:last-child{border-radius:0 3px 3px 0}.segment:only-child{border-radius:3px}
  .segment:hover:not(:disabled):not(.selected){box-shadow:inset 0 0 0 1px var(--accent-hover)}.segment:disabled{cursor:default}
  .segment:focus-visible{outline:2px solid var(--accent-hover);outline-offset:2px;z-index:1}
  /* A dark backing keeps labels readable on every fill (4.5:1 or more). */
  .text{position:relative;margin:0 6px;padding:1px 4px;border-radius:2px;background:rgb(19 20 28/.75);overflow:hidden;text-overflow:clip}
  .used{background:linear-gradient(to right,var(--border-strong) var(--used),var(--surface-raised) var(--used))}
  .dimmed{color:var(--text-dim)}.dimmed:not(.used){background:var(--border)}.outlined-dimmed{opacity:.6}
  .hatched{background:repeating-linear-gradient(135deg,var(--border) 0 2px,var(--surface-deep) 2px 8px)}
  .checking{background:linear-gradient(90deg,var(--surface-raised) 0%,var(--surface-hover) 50%,var(--surface-raised) 100%) 0 0/200% 100%;animation:shimmer 1.6s linear infinite}
  .outlined,.outlined-dimmed{background:transparent;border:1px dashed var(--border-strong)}
  .selected{box-shadow:inset 0 0 0 2px var(--accent);opacity:1}
  .source::after{content:'';position:absolute;left:0;right:0;bottom:0;height:3px;background:var(--accent)}
  @keyframes shimmer{from{background-position:100% 0}to{background-position:-100% 0}}
  @media(prefers-reduced-motion:reduce){.checking{animation:none}}
  @media(max-width:550px){.segment:not(.selected) .text{display:none}}
</style>
