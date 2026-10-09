<script lang="ts">
  import { cantReleaseLine, editorSegments, formatSize, freeTargetLine, GiB, windowsAfterBytes, type EditorSegment } from './stagedLayout';
  import type { StagedChoice, StagedRegion } from './setup';
  // The space source zoomed in: how the provider will lay out the selected
  // partition or free region, and Omarchy's size when Windows is kept.
  export let choice:StagedChoice;
  // Absent with an older provider that reports no disk layout.
  export let region:StagedRegion|undefined=undefined;
  export let targetName='Windows';
  export let temporaryBytes:number;
  export let minimumBytes:number;
  export let minGiB=0;
  export let maxGiB=0;
  export let canKeep=false;
  export let keep=true;
  export let omarchyGiB=64;
  export let disabled=false;
  // Called on every change, so the parent can fix the space source.
  export let edit:()=>void=()=>{};
  // The handle's width; the range input is widened by it so the handle's
  // center lands exactly where the installer starts.
  const CHAR_PX=6.6, PADDING_PX=20, THUMB_PX=14;
  const id=Math.random().toString(36).slice(2);
  let width=0;
  let explaining=false;
  $: shrink=choice.target.target_kind==='shrink';
  $: keeping=shrink && canKeep && keep;
  $: barPx=width || 760;
  $: segments=region ? placed(editorSegments(region,{temporaryBytes,omarchyBytes:omarchyGiB*GiB,mode:keeping ? 'keep' : 'replace',barPx})) : [];
  $: installer=segments.find(s=>s.kind==='temporary');
  // The handle moves between where the installer starts with the largest
  // and with the smallest Omarchy size.
  $: span=keeping && region ? [installerStart(region,maxGiB,barPx),installerStart(region,minGiB,barPx)] : null;
  $: windowsAfter=region?.kind==='partition' ? windowsAfterBytes(region,temporaryBytes,keeping ? omarchyGiB*GiB : 0) : 0;
  $: valueText=`Omarchy ${omarchyGiB} GiB, ${targetName} keeps ${formatSize(windowsAfter)}`;
  function placed(list:EditorSegment[]) {
    let left=0;
    return list.map(segment=>{
      const item={...segment,leftPercent:left};
      left+=segment.widthPercent;
      return item;
    });
  }
  function installerStart(source:StagedRegion,gibibytes:number,px:number) {
    return placed(editorSegments(source,{temporaryBytes,omarchyBytes:gibibytes*GiB,mode:'keep',barPx:px})).find(s=>s.kind==='temporary')?.leftPercent ?? 0;
  }
  // A label sits inside its segment when it fits; Omarchy's drops the size
  // first. The installer is always named in the legend below.
  function inside(segment:EditorSegment,px:number) {
    const labels=segment.kind==='temporary' ? [] : segment.kind==='omarchy' ? [segment.label,'Omarchy'] : [segment.label];
    return labels.find(text=>text && text.length*CHAR_PX+PADDING_PX<=segment.widthPercent/100*px) ?? '';
  }
  function slide(event:Event) {
    omarchyGiB=Number((event.currentTarget as HTMLInputElement).value);
    edit();
  }
  // Page Up and Page Down move 10 GiB; arrows, Home and End are native.
  function page(event:KeyboardEvent) {
    const step=event.key==='PageUp' ? 10 : event.key==='PageDown' ? -10 : 0;
    if(!step) return;
    event.preventDefault();
    omarchyGiB=Math.min(maxGiB,Math.max(minGiB,omarchyGiB+step));
    edit();
  }
  // Typing applies whole sizes within the limits. Leaving the field clamps
  // what was typed, so the field shows the size that will be submitted.
  function type(event:Event) {
    const value=Number((event.currentTarget as HTMLInputElement).value);
    if(Number.isInteger(value) && value>=minGiB && value<=maxGiB) {
      omarchyGiB=value;
      edit();
    }
  }
  function commit(event:Event) {
    const input=event.currentTarget as HTMLInputElement, value=Math.round(Number(input.value));
    if(input.value!=='' && Number.isFinite(value)) omarchyGiB=Math.min(maxGiB,Math.max(minGiB,value));
    input.value=String(omarchyGiB);
    edit();
  }
</script>

{#if region}
  <div class="bar" bind:clientWidth={width}>
    {#each segments as segment,index (segment.kind)}
      {@const text=inside(segment,width)}
      {#if segment.kind==='cant-release'}
        <button type="button" class="segment cant-release" class:last={index===segments.length-1} style:left="{segment.leftPercent}%" style:width="{segment.widthPercent}%" aria-label={`Can't release ${formatSize(segment.sizeBytes)}`} aria-expanded={explaining} aria-controls={`cant-release-${id}`} onclick={()=>{explaining=!explaining;}}>{#if text}<span class="text" aria-hidden="true">{text}</span>{/if}</button>
      {:else}
        <span class="segment {segment.kind}" class:last={index===segments.length-1} style:left="{segment.leftPercent}%" style:width="{segment.widthPercent}%" aria-hidden="true">{#if text}<span class="text">{text}</span>{/if}</span>
      {/if}
    {/each}
    {#if span}
      <input type="range" class="handle" dir="rtl" min={minGiB} max={maxGiB} step="1" value={omarchyGiB} aria-label="Space for Omarchy" aria-valuetext={valueText} {disabled} style:left={`calc(${span[0]}% - ${THUMB_PX/2}px)`} style:width={`calc(${span[1]-span[0]}% + ${THUMB_PX}px)`} oninput={slide} onkeydown={page}>
    {/if}
  </div>
  <div class="legend">
    {#if region.kind==='partition'}<span>{targetName} {formatSize(windowsAfter)}</span>{/if}
    {#if installer}<span><i class="swatch" aria-hidden="true"></i>{installer.label}</span>{/if}
  </div>
{/if}
{#if shrink && canKeep}
  <div class="controls">
    <fieldset class="toggle" {disabled}>
      <legend class="visually-hidden">{targetName}</legend>
      <label class:on={keep}><input type="radio" name={`windows-${id}`} value={true} bind:group={keep} onchange={edit}>Keep {targetName}</label>
      <label class:on={!keep}><input type="radio" name={`windows-${id}`} value={false} bind:group={keep} onchange={edit}>Replace {targetName}</label>
    </fieldset>
    {#if keep}<label class="size">Omarchy<input type="number" aria-label="Space for Omarchy" min={minGiB} max={maxGiB} step="1" value={omarchyGiB} {disabled} oninput={type} onchange={commit}><span>GiB · {minGiB}–{maxGiB}</span></label>{/if}
  </div>
  {#if !keep}<p>In the installer, delete {targetName} to make room for Omarchy.</p>{/if}
{:else if shrink}
  <p>{region ? `${targetName} can't give enough space to keep it. You can replace it in the installer.` : `In the installer, delete ${targetName} to make room for Omarchy.`}</p>
{:else}
  <p>{freeTargetLine(choice.largestFreeAfterStagingBytes,minimumBytes)}</p>
{/if}
{#if region}<p id={`cant-release-${id}`} hidden={!explaining}>{cantReleaseLine(region)}</p>{/if}

<style>
  .bar{position:relative;height:56px;margin:4px 0 6px}
  /* Each segment leaves a 2 px gap on its right inside its own width, so
     positions stay exact for the handle. */
  .segment{position:absolute;top:0;bottom:0;display:flex;align-items:center;justify-content:center;box-sizing:border-box;margin:0;padding:0;border:0;border-right:2px solid transparent;border-radius:0;background-clip:padding-box;color:var(--text);font:inherit;font-size:11px;white-space:nowrap;overflow:hidden}
  .segment:first-child{border-radius:3px 0 0 3px}.segment.last{border-right:0;border-radius:0 3px 3px 0}
  .text{margin:0 6px;padding:1px 4px;border-radius:2px;background:rgb(19 20 28/.75);overflow:hidden}
  .used{background-color:var(--border-strong)}
  /* Used space is drawn short; the break marks it as not to scale. */
  .used::after{content:'//';position:absolute;right:8px;color:var(--surface-deep);font-weight:700;letter-spacing:-1px}
  .cant-release{background:repeating-linear-gradient(135deg,var(--border) 0 2px,var(--surface-deep) 2px 8px);background-clip:padding-box;cursor:pointer;appearance:none}
  .cant-release:hover{filter:brightness(1.2)}.cant-release:focus-visible{outline:2px solid var(--accent-hover);outline-offset:2px;z-index:1}
  .cant-release[aria-expanded='true']{box-shadow:inset 0 0 0 2px var(--accent)}
  .free{background-color:var(--surface-raised)}
  .temporary,.swatch{background:repeating-linear-gradient(45deg,var(--accent-hover) 0 3px,var(--surface-deep) 3px 7px);background-clip:padding-box}
  .omarchy{background-color:var(--accent)}
  .handle{position:absolute;top:-5px;height:66px;margin:0;padding:0;background:transparent;appearance:none;-webkit-appearance:none;cursor:ew-resize;z-index:2}
  .handle:disabled{cursor:default}
  .handle::-webkit-slider-runnable-track{height:66px;background:transparent}
  .handle::-webkit-slider-thumb{-webkit-appearance:none;appearance:none;width:14px;height:66px;border:2px solid var(--surface-deep);border-radius:4px;background:var(--text) linear-gradient(var(--surface-deep),var(--surface-deep)) center/2px 22px no-repeat}
  .handle::-moz-range-thumb{width:10px;height:62px;border:2px solid var(--surface-deep);border-radius:4px;background:var(--text)}
  .handle:focus-visible{outline:none}.handle:focus-visible::-webkit-slider-thumb{outline:2px solid var(--accent-hover);outline-offset:1px}
  .handle:disabled::-webkit-slider-thumb{opacity:.5}
  .legend{display:flex;flex-wrap:wrap;justify-content:space-between;gap:2px 16px;margin-bottom:10px;font-size:11px;color:var(--text-dim)}
  .swatch{display:inline-block;width:14px;height:10px;margin-right:6px;border-radius:2px;vertical-align:-1px}
  .controls{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:10px 24px;margin:0 0 8px}
  .toggle{display:inline-flex;gap:2px;margin:0;padding:2px;border:1px solid var(--border-strong);border-radius:5px}
  .toggle label{position:relative;display:flex;align-items:center;min-height:30px;padding:0 14px;border-radius:3px;font-size:12px;color:var(--text-muted);cursor:pointer}
  .toggle label.on{background:var(--surface-hover);color:var(--text);box-shadow:inset 0 0 0 1px var(--accent)}
  .toggle input{position:absolute;inset:0;width:100%;height:100%;margin:0;opacity:0;cursor:inherit}
  .toggle label:has(input:focus-visible){outline:2px solid var(--accent-hover);outline-offset:1px}
  .toggle:disabled label{opacity:.5;cursor:not-allowed}
  .size{display:flex;align-items:center;gap:8px;font-size:12px;color:var(--text)}
  .size input{width:84px;padding:4px 8px;background:var(--surface-raised);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:20px;font-weight:700}
  .size span{color:var(--text-dim)}
  p{margin:0 0 8px;font-size:12px;line-height:1.8;color:var(--text-muted)}
  p[hidden]{display:none}
  .visually-hidden{position:absolute;width:1px;height:1px;margin:-1px;padding:0;border:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
</style>
