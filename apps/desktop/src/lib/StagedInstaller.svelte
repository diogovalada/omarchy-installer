<script lang="ts">
  import { ArrowLeft, ArrowRight, HardDrive, Check, RefreshCw } from 'lucide-svelte';
  import BitLockerPreparation from './BitLockerPreparation.svelte';
  import { setup, setupActive, type SetupSnapshot, type StagedBlockedDisk } from './setup';
  export let recoveryOnly=false;
  export let close:()=>void=()=>{};
  export let sourceReady=false;
  export let prepareAgain:()=>void=()=>{};
  let selectedDisk='';
  let selectedSpace='';
  let inspection:SetupSnapshot['stagedIso'];
  let linuxGiB=40;
  let previousChoice:string|undefined;
  let restarting:string|null=null;
  $: busy=$setup.pending || setupActive($setup.snapshot?.status);
  $: result=$setup.snapshot?.stagedIso;
  // Keep the disk list visible while the helper runs another operation.
  $: if(result?.choices && !busy && !$setup.error && !$setup.snapshot?.error) inspection=result;
  $: disks=Array.from(new Map([
    ...(inspection?.choices ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:undefined as StagedBlockedDisk|undefined}] as const),
    ...(inspection?.blocked ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:c}] as const)
  ]).values()).sort((a,b)=>a.number-b.number);
  $: disk=disks.find(d=>d.id===selectedDisk);
  $: spaces=(inspection?.choices ?? []).filter(c=>c.diskUniqueId===selectedDisk && !disk?.blocked);
  $: choice=spaces.find(c=>JSON.stringify(c.target)===selectedSpace) ?? spaces[0];
  $: operations=($setup.snapshot?.stagedRecovery?.operations ?? result?.operations ?? []).filter(op=>op.status!=='cleaned' && !(result?.operationId===op.operationId && result.status==='cleaned'));
  $: recordErrors=$setup.snapshot?.stagedRecovery?.recordErrors ?? result?.recordErrors ?? [];
  $: testing=$setup.snapshot?.stagedTesting===true;
  $: error=$setup.error ?? $setup.snapshot?.error;
  $: if(restarting && !busy && result?.operationId===restarting && result.status==='cleaned') {
    restarting=null; selectedDisk=''; prepareAgain(); inspect();
  }
  $: if(error) restarting=null;
  $: minimum=Math.ceil((inspection?.minimumLinuxBytes ?? 40*1024**3)/1024**3);
  $: maximum=choice ? Math.floor(choice.maximumLinuxBytes/1024**3) : 0;
  $: choiceKey=JSON.stringify(choice);
  $: if(choiceKey!==previousChoice){previousChoice=choiceKey;if(choice)linuxGiB=Math.floor(choice.maximumLinuxBytes/1024**3);}
  function inspect() {
    inspection=undefined; selectedSpace='';
    void setup.stagedIso('inspect');
  }
  async function stage() {
    if(!testing || !sourceReady || !choice || busy || !Number.isInteger(linuxGiB) || linuxGiB<minimum || linuxGiB>maximum) return;
    await setup.stagedIso('stage',{diskNumber:choice.diskNumber,diskUniqueId:choice.diskUniqueId,target:choice.target,linuxBytes:linuxGiB*1024**3});
  }
</script>

{#if !recoveryOnly || operations.length || recordErrors.length}
<section class="staged" aria-label="Temporary installer">
  {#if !recoveryOnly}
    <div class="navigation"><button class="back" disabled={busy} onclick={close}><ArrowLeft size={15}/>Back</button>{#if testing}<span class="badge">Experimental</span>{/if}</div>
    <h2>Install without USB</h2>
    <p class="intro">Make room for Omarchy. Restart to finish installing.</p>
    {#if testing}<details class="testing"><summary>Test build · real disk changes</summary><p role="note">This makes real disk changes. Completing installation requires an official ISO with same-disk and suspended-BitLocker support.</p></details>{/if}
    <div class="destination">
      <div class="section-heading"><h3>Choose a disk</h3>{#if inspection}<button class="refresh" disabled={busy || !sourceReady || !testing} onclick={inspect}><RefreshCw size={14}/>Refresh</button>{/if}</div>
      {#if !sourceReady}
        <p class="empty">Download the ISO above to see available disks.</p>
      {:else if !inspection}
        <p class="empty">Check your disks and available space. Windows will ask for administrator access.</p>
      {/if}
      {#if !inspection}<button class="primary" disabled={busy || !testing || !sourceReady} onclick={inspect}><HardDrive size={16}/>Check available space</button>{/if}
      {#if inspection}
        <div class="disks" aria-label="Destination disk">
          {#each disks as item}
            <button class="disk" class:selected={selectedDisk===item.id} aria-pressed={selectedDisk===item.id} disabled={busy || !sourceReady} onclick={()=>{selectedDisk=item.id;selectedSpace='';}}>
              <HardDrive size={22}/><span class="disk-copy"><strong>Disk {item.number}{#if item.size} · {Math.round(item.size/1024**3)} GiB{/if}</strong><span>{item.blocked ? 'Needs attention' : 'Space available'}</span></span>
              {#if selectedDisk===item.id}<Check size={17}/>{/if}
            </button>
          {/each}
        </div>
        {#if !disks.length}<p class="empty">No suitable disks found. Check Windows Disk Management, then refresh.</p>{/if}
      {/if}
    </div>
    {#if disk?.blocked}
      <div class="blocked" role="status"><h3>This disk needs attention</h3><p>{disk.blocked.reason}</p></div>
    {:else if choice}
      <div class="allocation">
        <div class="section-heading"><h3>Space for Omarchy</h3><span>{minimum}–{maximum} GiB available</span></div>
        {#if spaces.length>1}<label class="space-source">Use space from<select value={JSON.stringify(choice.target)} onchange={event=>{selectedSpace=event.currentTarget.value;}} disabled={busy}>{#each spaces as space}<option value={JSON.stringify(space.target)}>{space.label} · {Math.floor(space.maximumLinuxBytes/1024**3)} GiB</option>{/each}</select></label>{:else}<p class="space-source">{choice.label}</p>{/if}
        <div class="size-input"><input aria-label="Linux space (GiB)" type="number" min={minimum} max={maximum} step="1" bind:value={linuxGiB} disabled={busy}/><span>GiB</span></div>
        <p class="allocation-note">+ {((inspection?.temporaryBytes ?? 0)/1024**3).toFixed(1)} GiB for the temporary installer</p>
        <details class="space-details"><summary>How is this space used?</summary><p>{choice.target.target_kind==='shrink' ? 'Windows will shrink the selected partition to make room. Existing files are kept.' : 'Existing partitions are kept. Only the selected unallocated space is used.'} The temporary installer can be removed after installation, leaving its space unallocated.</p></details>
        {#if choice.encryption?.some(volume=>volume.protectionStatus===1)}<BitLockerPreparation/>{/if}
        <div class="continue"><span>Review changes before applying</span><button class="primary" disabled={busy || !testing || !sourceReady || !Number.isInteger(linuxGiB) || linuxGiB<minimum || linuxGiB>maximum} onclick={stage}>Review changes<ArrowRight size={16}/></button></div>
      </div>
    {/if}
  {/if}
  {#each recordErrors as record}
    <p role="alert">A previous temporary installer could not be identified safely. Its partitions have not been changed.</p>
    <details><summary>Details</summary><p>{record.operationId}: {record.message}</p></details>
  {/each}
  {#each operations as op}
    <article>
      <h3>{op.status==='boot-scheduled' ? 'Installer startup requested' : ['staged','arming'].includes(op.status) ? 'Temporary installer ready' : 'Installer preparation needs attention'}</h3>
      <p>Disk {op.diskNumber} · {(op.temporaryBytes/1024**3).toFixed(1)} GiB temporary storage</p>
      <details><summary>When should I remove it?</summary><p>Remove it after Omarchy boots independently, or to abandon this attempt. Windows and installed Omarchy are kept. These actions require administrator approval.</p></details>
      <div class="operation-actions">{#if testing && ['staged','arming'].includes(op.status)}<button class="primary" disabled={busy} onclick={()=>{void setup.stagedIso('arm',null,op.operationId);}}>Start installer on next restart</button>{/if}
      <button disabled={busy} onclick={()=>{void setup.stagedIso('cleanup',null,op.operationId);}}>Remove temporary installer</button>
      {#if testing && sourceReady}<button disabled={busy} onclick={()=>{restarting=op.operationId;void setup.stagedIso('cleanup',null,op.operationId);}}>Prepare again</button>{/if}</div>
    </article>
  {/each}
  {#if busy}<p role="status">{$setup.snapshot?.message || 'Checking…'}</p>{/if}
  {#if !busy && result?.message}<p role="status">{result.message}</p>{/if}
  {#if !busy && $setup.snapshot?.bitLocker}<p role="status">{$setup.snapshot.bitLocker.message}</p>{/if}
  {#if error}<div class="failure" role="alert"><strong>Couldn’t complete this step.</strong><details><summary>Show details</summary><p>{error}</p></details></div>{/if}
</section>
{/if}

<style>
  .staged{margin-top:22px}h2{font-size:22px;letter-spacing:-.6px;margin:18px 0 8px}h3{font-size:13px;margin:0}p{font-size:11px;line-height:1.8;color:var(--text-muted)}.intro{margin:0;font-size:12px}
  .navigation,.section-heading{display:flex;align-items:center;justify-content:space-between;gap:16px}.badge{font-size:10px;color:var(--accent);background:var(--surface-raised);border:1px solid var(--border);border-radius:20px;padding:5px 10px}
  button{display:inline-flex;align-items:center;justify-content:center;gap:9px;min-height:42px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:hover:not(:disabled){border-color:var(--accent);background:var(--surface-hover)}button:disabled{opacity:.5;cursor:not-allowed}
  .back,.refresh{border-color:transparent;background:transparent;padding:6px 0;min-height:32px;color:var(--text-dim)}.back:hover:not(:disabled),.refresh:hover:not(:disabled){background:transparent;border-color:transparent;color:var(--accent)}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  details{font-size:11px;color:var(--text-dim);line-height:1.8}summary{cursor:pointer;padding:4px 0}.testing{margin-top:12px}.testing p{margin:6px 0}.destination{margin:24px 0 16px}.section-heading{margin-bottom:12px}.section-heading>span{font-size:10px;color:var(--text-dim)}.empty{margin:12px 0 16px}
  .disks{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px}.disk{justify-content:flex-start;padding:18px;gap:14px;text-align:left;border-color:var(--border);background:var(--surface);min-height:84px}.disk.selected{background:var(--surface-raised);border-color:var(--accent);box-shadow:inset 0 0 0 1px var(--accent)}.disk.selected :global(svg){color:var(--accent)}.disk-copy{flex:1;display:flex;flex-direction:column;gap:6px}.disk-copy strong{font-size:13px}.disk-copy>span{font-size:10px;color:var(--text-dim)}
  .allocation,.blocked,article{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:22px;margin:16px 0}.space-source{display:block;font-size:11px;color:var(--text-muted);margin:0 0 16px}select{display:block;width:100%;margin-top:8px;padding:12px;background:var(--surface-raised);border:1px solid var(--border);color:var(--text);border-radius:4px;font-size:11px}.size-input{display:flex;align-items:center;gap:12px}.size-input input{width:155px;max-width:100%;font-size:24px;font-weight:700;padding:12px;background:var(--surface-deep);border:1px solid var(--border-strong);border-radius:4px;color:var(--text)}.size-input>span{font-size:12px;color:var(--text-dim)}.allocation-note{margin:12px 0}.space-details{margin-bottom:18px}.continue{border-top:1px solid var(--border);padding-top:18px;display:flex;align-items:center;justify-content:space-between;gap:16px}.continue>span{font-size:10px;color:var(--text-dim)}.operation-actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:14px}.failure{border-left:2px solid var(--error);padding:12px 16px;font-size:12px;margin:16px 0}.failure p{overflow-wrap:anywhere}
  @media(max-width:550px){h2{font-size:20px}.allocation,.blocked,article{padding:16px}.continue{align-items:stretch;flex-direction:column;gap:10px}.section-heading{flex-wrap:wrap;gap:8px}.disks{grid-template-columns:1fr}}
</style>
