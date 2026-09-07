<script lang="ts">
  import { formatBytes } from './downloads';
  import DeletePartitionModal from './DeletePartitionModal.svelte';
  import type { StorageChoice } from './storageChoices';
  export let choices:StorageChoice[];
  export let prepare:(choice:StorageChoice,bytes:number|undefined,confirmation?:string)=>void;
  export let helpTitle:string;
  export let helpText:string;
  let selected='';
  let sizedChoice='';
  let sizeGiB='';
  let showDeletion=false;
  let deleteChoice:StorageChoice|null=null;
  $: choice=choices.find(item=>item.id===selected && item.eligible);
  $: allocation=choice?.allocation;
  $: if (choice?.id!==sizedChoice) {
    sizedChoice=choice?.id ?? '';
    sizeGiB=allocation?.recommendedBytes ? String(allocation.recommendedBytes/1024**3) : '';
  }
  $: requestedBytes=/^\d+(?:\.\d+)?$/.test(sizeGiB) ? Math.floor(Number(sizeGiB)*1024)*1024**2 : 0;
  $: recommended=allocation?.mode==='automatic' && sizeGiB.trim()==='';
  $: validSize=!allocation || recommended || (Number.isSafeInteger(requestedBytes) && requestedBytes>0 && requestedBytes>=(allocation.minimumBytes ?? 1) && requestedBytes<=(allocation.maximumBytes ?? Number.MAX_SAFE_INTEGER));
  $: remainingBytes=allocation ? (allocation.mode==='shrink' ? Math.floor((allocation.currentPartitionBytes ?? 0)/1024**2)*1024**2 : allocation.maximumBytes ?? 0)-requestedBytes : 0;
  $: if (deleteChoice && JSON.stringify(deleteChoice)!==JSON.stringify(choice)) deleteChoice=null;
  function submit() {
    if (!choice || !validSize) return;
    if (choice.deletion) deleteChoice=choice;
    else prepare(choice,allocation && !recommended ? requestedBytes : undefined);
  }
  function confirm(identifier:string) {
    if (!deleteChoice || !choice || JSON.stringify(deleteChoice)!==JSON.stringify(choice) || !validSize) return;
    deleteChoice=null;
    prepare(choice,allocation && !recommended ? requestedBytes : undefined,identifier);
  }
</script>

{#if choices.some(item=>item.deletion)}
  <button class="replacement-toggle" aria-expanded={showDeletion} onclick={()=>{showDeletion=!showDeletion;if(!showDeletion && choice?.deletion) selected='';}}>{showDeletion ? 'Hide replacement options' : 'Replace existing storage…'}</button>
{/if}
<div class="choices">
  {#each choices.filter(item=>!item.deletion || showDeletion) as item}
    <label class:unavailable={!item.eligible} class:selected={selected===item.id}>
      <input type="radio" name="storage-target" value={item.id} bind:group={selected} disabled={!item.eligible}/>
      <span><strong>{item.label}</strong><small>{item.detail}{#if item.sizeDescription || item.sizeBytes!==undefined} · {item.sizeDescription ?? formatBytes(item.sizeBytes!)}{/if}</small>{#each item.reasons as reason}<small class="reason">{reason}</small>{/each}</span>
    </label>
  {/each}
</div>
{#if allocation}
  <div class="allocation">
    <label for="omarchy-size">Space for Omarchy (GiB)</label>
    <div class="size-controls"><input id="omarchy-size" type="text" inputmode="decimal" placeholder="Recommended" bind:value={sizeGiB} aria-describedby="size-limits" aria-invalid={!validSize}/>{#if allocation.maximumBytes!==undefined}<button onclick={()=>{sizeGiB=String(allocation!.maximumBytes!/1024**3);}}>Use maximum</button>{/if}</div>
    <p id="size-limits" class="muted">{#if allocation.minimumBytes!==undefined && allocation.maximumBytes!==undefined}{formatBytes(allocation.minimumBytes)} minimum · {formatBytes(allocation.maximumBytes)} maximum{:else}Leave blank for the native installer’s recommendation. It checks available space before preparing your plan.{/if}</p>
    {#if !validSize}<p class="error">Choose a positive amount within the available limits.</p>
    {:else if allocation.mode==='shrink'}<p>{allocation.windowsVolume ?? 'Windows'} · {formatBytes(allocation.currentPartitionBytes ?? 0)} → {formatBytes(remainingBytes)}</p>
    {:else if allocation.maximumBytes!==undefined}<p>{formatBytes(remainingBytes)} will remain unallocated.</p>{/if}
  </div>
{/if}
{#if choice?.deletion}<p class="error">The entire {choice.deletion.scope==='installation' ? 'existing installation' : 'partition'} ({choice.deletion.sizeDescription ?? formatBytes(choice.deletion.sizeBytes ?? 0)}) and its data will be deleted.{#if allocation} This also applies if you allocate less space to Omarchy.{/if} Confirm its identifier to continue.</p>{/if}
{#if choice}<slot/><button class="primary prepare" disabled={!validSize} onclick={submit}>{choice.deletion ? 'Review deletion' : 'Prepare installation'}</button>{/if}
<details class="resize-help"><summary>{helpTitle}</summary><p>{helpText}</p></details>
{#if deleteChoice}<DeletePartitionModal choice={deleteChoice} confirm={confirm} close={()=>{deleteChoice=null;}}/>{/if}

<style>
  p{font-size:11px;line-height:1.8;color:var(--text-muted);margin:12px 0}.muted{font-size:10px;color:var(--text-dim)}.error{color:var(--error)}.resize-help{margin:18px 0}summary{font-size:10px;color:var(--text-muted);cursor:pointer}summary:focus-visible{outline:2px solid var(--accent);outline-offset:4px}.replacement-toggle{margin-bottom:16px}
  button{display:inline-flex;align-items:center;justify-content:center;gap:8px;min-height:38px;padding:9px 13px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:disabled{opacity:.5;cursor:not-allowed}button:hover:not(:disabled){border-color:var(--accent)}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep)}.prepare{margin-top:18px}
  .choices{display:grid;gap:10px}.choices label{display:flex;align-items:flex-start;gap:12px;padding:14px;border:1px solid var(--border);border-radius:3px;cursor:pointer}.choices label.selected{border-color:var(--accent)}.choices label.unavailable{cursor:not-allowed}.choices input{margin-top:4px;accent-color:var(--accent)}.choices span{min-width:0}.choices strong{font-size:12px;font-weight:400;overflow-wrap:anywhere}.choices small{display:block;color:var(--text-muted);font-size:10px;line-height:1.8;margin-top:5px}.choices small.reason{color:var(--text-dim)}.unavailable strong{color:var(--text-muted)}
  .allocation{margin-top:24px;padding-top:20px;border-top:1px solid var(--border)}.allocation label{font-size:11px;color:var(--text-muted)}.size-controls{display:flex;gap:10px;margin-top:10px}.size-controls input{width:150px;min-width:0;padding:9px 12px;border:1px solid var(--border-strong);border-radius:3px;background:var(--surface-deep);color:var(--text);font:inherit;font-size:12px}.size-controls input[aria-invalid=true]{border-color:var(--error)}
</style>
