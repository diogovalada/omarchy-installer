<script lang="ts">
  import { ArrowLeft, ArrowRight, HardDrive, Check, RefreshCw } from 'lucide-svelte';
  import BitLockerPreparation from './BitLockerPreparation.svelte';
  import { setup, setupActive, type SetupSnapshot, type StagedBlockedDisk, type StagedDisk, type StagedRegion, type StagedResizeQuery } from './setup';
  export let recoveryOnly=false;
  export let close:()=>void=()=>{};
  export let sourceReady=false;
  export let prepareAgain:()=>void=()=>{};
  let selectedDisk='';
  let selectedSpace='';
  let inspection:SetupSnapshot['stagedIso'];
  let restarting:string|null=null;
  let handledInspection='';
  let analyzing:StagedResizeQuery|null=null;
  $: busy=$setup.pending || setupActive($setup.snapshot?.status);
  $: result=$setup.snapshot?.stagedIso;
  $: error=$setup.error ?? $setup.snapshot?.error;
  $: browsing=!busy || ($setup.snapshot?.status==='running' && ['inspecting','analyzing-resize'].includes($setup.snapshot?.stage) && !!result?.disks);
  $: if(!result && busy) handledInspection='';
  // The elevated check sends an initial layout, then updated resize limits.
  // Polling can return a new object every second; apply each version once.
  $: if(result?.disks && !error) {
    const signature=JSON.stringify({disks:result.disks,choices:result.choices,blocked:result.blocked});
    if(signature!==handledInspection) {
      handledInspection=signature;
      inspection=analyzing && !busy && inspection ? mergeInspection(inspection,result,analyzing) : result;
      analyzing=null;
    }
  } else if(result?.choices && !result.disks && !busy && !error) {
    const signature=JSON.stringify({choices:result.choices,blocked:result.blocked});
    if(signature!==handledInspection) { handledInspection=signature; inspection=result; }
  }
  $: if(error && analyzing) analyzing=null;
  $: disks=Array.from(new Map([
    ...(inspection?.disks ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:undefined as StagedBlockedDisk|undefined}] as const),
    ...(inspection?.choices ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:undefined as StagedBlockedDisk|undefined}] as const),
    ...(inspection?.blocked ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:c}] as const)
  ]).values()).sort((a,b)=>a.number-b.number);
  $: disk=disks.find(d=>d.id===selectedDisk);
  $: if(inspection?.disks?.length && !disks.some(d=>d.id===selectedDisk)) selectedDisk=disks.find(d=>!d.blocked)?.id ?? disks[0]?.id ?? '';
  $: layout=inspection?.disks?.find(d=>d.diskUniqueId===selectedDisk);
  $: listedRegions=layout?.regions.filter(r=>r.kind!=='free' || r.sizeBytes>=5*1024**2) ?? [];
  $: smallGapBytes=layout?.regions.filter(r=>r.kind==='free' && r.sizeBytes<5*1024**2).reduce((total,r)=>total+r.sizeBytes,0) ?? 0;
  $: requiredBytes=inspection?.temporaryBytes ?? 0;
  $: installMinimum=inspection?.minimumLinuxBytes ?? 32*1024**3;
  $: spaces=(inspection?.choices ?? []).filter(c=>c.diskUniqueId===selectedDisk && !disk?.blocked);
  $: choice=spaces.find(c=>JSON.stringify(c.target)===selectedSpace) ?? spaces[0];
  $: operations=($setup.snapshot?.stagedRecovery?.operations ?? result?.operations ?? []).filter(op=>op.status!=='cleaned' && !(result?.operationId===op.operationId && result.status==='cleaned'));
  $: recordErrors=$setup.snapshot?.stagedRecovery?.recordErrors ?? result?.recordErrors ?? [];
  $: testing=$setup.snapshot?.stagedTesting===true;
  $: if(restarting && !busy && result?.operationId===restarting && result.status==='cleaned') {
    restarting=null; selectedDisk=''; prepareAgain(); inspect();
  }
  $: if(error) restarting=null;
  function sameLayout(before:StagedDisk,after:StagedDisk) {
    return before.diskSizeBytes===after.diskSizeBytes && before.regions.length===after.regions.length && before.regions.every((region,index)=>{
      const current=after.regions[index];
      return region.kind===current.kind && region.offsetBytes===current.offsetBytes && region.sizeBytes===current.sizeBytes && region.partitionGuid===current.partitionGuid;
    });
  }
  function mergeInspection(previous:NonNullable<SetupSnapshot['stagedIso']>,current:NonNullable<SetupSnapshot['stagedIso']>,query:StagedResizeQuery):NonNullable<SetupSnapshot['stagedIso']> {
    const oldDisks=new Map((previous.disks ?? []).map(d=>[d.diskUniqueId,d]));
    const disks=(current.disks ?? []).map(latest=>{
      const old=oldDisks.get(latest.diskUniqueId);
      if(!old || !sameLayout(old,latest)) return latest;
      return {...latest,regions:latest.regions.map(region=>{
        if(latest.diskUniqueId===query.diskUniqueId && region.partitionNumber===query.partitionNumber && region.partitionGuid===query.partitionGuid) return region;
        const saved=old.regions.find(r=>r.partitionGuid===region.partitionGuid && r.offsetBytes===region.offsetBytes && r.sizeBytes===region.sizeBytes);
        return saved?.resizeState==='checked' && saved.freeBytes===region.freeBytes ? {...region,resizeState:saved.resizeState,resizeReason:saved.resizeReason,maximumReleaseBytes:saved.maximumReleaseBytes,reserveBytes:saved.reserveBytes} : region;
      })};
    });
    const choices=[...(current.choices ?? [])];
    for(const old of previous.choices ?? []) {
      if(old.target.target_kind!=='shrink' || choices.some(c=>JSON.stringify(c.target)===JSON.stringify(old.target))) continue;
      const disk=disks.find(d=>d.diskUniqueId===old.diskUniqueId);
      const region=disk?.regions.find(r=>r.partitionGuid===old.target.partition_guid && r.resizeState==='checked');
      if(region && oldDisks.get(old.diskUniqueId)?.regions.some(r=>r.partitionGuid===region.partitionGuid && r.freeBytes===region.freeBytes)) choices.push({...old,encryption:disk?.encryption});
    }
    return {...current,disks,choices};
  }
  function inspect() {
    inspection=undefined; selectedSpace=''; analyzing=null;
    void setup.stagedIso('inspect');
  }
  function size(bytes:number) {
    return bytes<1024**3 ? `${Math.floor(bytes/1024**2)} MiB` : `${(bytes/1024**3).toFixed(1)} GiB`;
  }
  function analyze(region:StagedRegion) {
    if(!disk || !layout || busy || !testing || !sourceReady || !region.partitionNumber || !region.partitionGuid) return;
    const query={diskNumber:disk.number,diskUniqueId:disk.id,partitionNumber:region.partitionNumber,partitionGuid:region.partitionGuid};
    analyzing=query;
    void setup.stagedIso('inspect',null,null,query);
  }
  async function stage() {
    if(!testing || !sourceReady || !choice || busy || error) return;
    await setup.stagedIso('stage',{diskNumber:choice.diskNumber,diskUniqueId:choice.diskUniqueId,target:choice.target});
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
            <button class="disk" class:selected={selectedDisk===item.id} aria-pressed={selectedDisk===item.id} disabled={!sourceReady || !browsing} onclick={()=>{selectedDisk=item.id;selectedSpace='';}}>
              <HardDrive size={22}/><span class="disk-copy"><strong>Disk {item.number}{#if item.size} · {Math.round(item.size/1024**3)} GiB{/if}</strong><span>{item.blocked ? 'Unavailable for installation' : 'View partitions and resize options'}</span></span>
              {#if selectedDisk===item.id}<Check size={17}/>{/if}
            </button>
          {/each}
        </div>
        {#if !disks.length}<p class="empty">No suitable disks found. Check Windows Disk Management, then refresh.</p>{/if}
      {/if}
    </div>
    {#if layout}
      <div class="partition-panel">
        <div class="section-heading"><h3>Partitions on Disk {layout.diskNumber}</h3><span>{size(layout.unallocatedBytes)} unallocated</span></div>
        <div class="space-budget"><strong>{size(requiredBytes)} needed</strong><span>For temporary installer files and an EFI loader. The booted installer handles Omarchy's space.</span></div>
        <div class="disk-map" aria-hidden="true">{#each layout.regions as region}<span class:free={region.kind==='free'} style:flex-grow={region.sizeBytes} title={`${region.label} · ${size(region.sizeBytes)}`}></span>{/each}</div>
        <ul class="partitions" aria-label="Partitions and unallocated space">
          {#each listedRegions as region}
            <li>
              <div class="partition-heading"><strong>{region.label}{#if region.fileSystem}<small>{region.fileSystem}</small>{/if}</strong><span>{size(region.sizeBytes)}</span></div>
              {#if region.kind==='free'}
                <p>{region.sizeBytes>=requiredBytes ? 'Enough room for the temporary installer.' : `${size(requiredBytes-region.sizeBytes)} more needed for the temporary installer.`}</p>
              {:else}
                {#if region.freeBytes!=null}<p>{size(region.freeBytes)} unused inside this partition</p>{/if}
                {#if region.resizeState==='unchecked'}
                  <p>{busy ? 'Checking Windows resize limit…' : 'Resize limit was not checked. Refresh to try again.'}</p>
                {:else if region.resizeState==='insufficient'}
                  {#if (region.freeBytes ?? 0)<requiredBytes}<p>{size(requiredBytes-(region.freeBytes ?? 0))} more unused space needed in this partition. Free up space in Windows, then refresh.</p>{:else}<p>{region.resizeReason}</p>{/if}
                {:else if region.resizeState==='checked'}
                  <p class="resize-result">Can release {size(region.maximumReleaseBytes ?? 0)}{#if region.reserveBytes} · keeps {size(region.reserveBytes)} free{/if}</p>
                  {#if (region.maximumReleaseBytes ?? 0)<requiredBytes}<p>{size(requiredBytes-(region.maximumReleaseBytes ?? 0))} short of the space needed.</p>{/if}
                  {#if region.resizeReason}<p>{region.resizeReason}</p>{/if}
                  {#if (region.maximumReleaseBytes ?? 0)<requiredBytes}<details><summary>Why can’t all unused space be used?</summary><p>Windows may have files it cannot move near the end of the partition. We also keep free space for Windows. Free up files or adjust the volume in Windows, then check again.</p></details>{/if}
                  <button class="resize" disabled={busy || !testing || !sourceReady} onclick={()=>analyze(region)}>Recheck {region.label}</button>
                {:else if region.resizeState==='failed'}<p>{region.resizeReason || 'Windows could not measure the resize limit.'}</p><button class="resize" disabled={busy || !testing || !sourceReady} onclick={()=>analyze(region)}>Recheck {region.label}</button>
                {:else}<p>{region.resizeReason}</p>{/if}
              {/if}
            </li>
          {/each}
        </ul>
        {#if smallGapBytes>0}<details class="space-details"><summary>{size(smallGapBytes)} in small alignment gaps</summary><p>These gaps remain in the disk layout and unallocated total. Each is too small for the temporary installer.</p></details>{/if}
        {#if !choice}<p class="space-hint">{busy && layout.regions.some(r=>r.resizeState==='unchecked') ? 'Windows is checking how much space it can release. This does not change your disk.' : 'No region is large enough yet. You can recheck resize limits or refresh after freeing space in Windows.'}</p>{/if}
      </div>
    {/if}
    {#if disk?.blocked}
      <div class="blocked" role="status"><h3>This disk needs attention</h3><p>{disk.blocked.reason}</p></div>
    {:else if choice}
      <div class="allocation">
        <div class="section-heading"><h3>Temporary installer</h3><span>{size(requiredBytes)}</span></div>
        {#if spaces.length>1}<label class="space-source">Use space from<select value={JSON.stringify(choice.target)} onchange={event=>{selectedSpace=event.currentTarget.value;}} disabled={!browsing}>{#each spaces as space}<option value={JSON.stringify(space.target)}>{space.label}</option>{/each}</select></label>{:else}<p class="space-source">{choice.label}</p>{/if}
        {#if choice.largestFreeAfterStagingBytes<installMinimum}<div class="space-warning" role="note"><strong>More space needed to finish</strong><p>The largest free region after staging will be {size(choice.largestFreeAfterStagingBytes)}; Omarchy needs at least {size(installMinimum)}. A future official ISO with same-disk support will let you delete an unneeded partition after boot.</p></div>{/if}
        <details class="space-details"><summary>How is this space used?</summary><p>{choice.target.target_kind==='shrink' ? 'Windows will shrink the selected partition to make room. Existing files are kept.' : 'Existing partitions are kept. Only the selected unallocated space is used.'} The temporary installer can be removed after installation, leaving its space unallocated.</p></details>
        {#if choice.encryption?.some(volume=>volume.protectionStatus===1)}<BitLockerPreparation/>{/if}
        <div class="continue"><span>Review changes before applying</span><button class="primary" disabled={busy || !!error || !testing || !sourceReady} onclick={stage}>Review changes<ArrowRight size={16}/></button></div>
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
  .partition-panel{border:1px solid var(--border);border-radius:6px;padding:22px;background:var(--surface)}.space-budget{display:flex;flex-wrap:wrap;align-items:baseline;gap:8px 18px;margin:16px 0;font-size:12px}.space-budget>span{font-size:10px;color:var(--text-dim)}.disk-map{display:flex;gap:3px;height:14px;margin:20px 0;border-radius:3px;overflow:hidden}.disk-map>span{min-width:4px;background:var(--accent);opacity:.7}.disk-map>span.free{background:var(--text-dim);opacity:.25}.partitions{list-style:none;margin:0;padding:0}.partitions li{padding:16px 0;border-bottom:1px solid var(--border)}.partitions li:last-child{border-bottom:0}.partition-heading{display:flex;justify-content:space-between;gap:12px;font-size:12px}.partition-heading strong{display:flex;align-items:baseline;flex-wrap:wrap;gap:10px}.partition-heading small{color:var(--text-dim);font-size:10px;font-weight:400}.partitions p{margin:6px 0}.resize{margin-top:8px}.space-hint{border-top:1px solid var(--border);padding-top:12px}.resize-result{color:var(--text)}
  .staged{margin-top:22px}h2{font-size:22px;letter-spacing:-.6px;margin:18px 0 8px}h3{font-size:13px;margin:0}p{font-size:11px;line-height:1.8;color:var(--text-muted)}.intro{margin:0;font-size:12px}
  .navigation,.section-heading{display:flex;align-items:center;justify-content:space-between;gap:16px}.badge{font-size:10px;color:var(--accent);background:var(--surface-raised);border:1px solid var(--border);border-radius:20px;padding:5px 10px}
  button{display:inline-flex;align-items:center;justify-content:center;gap:9px;min-height:42px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:hover:not(:disabled){border-color:var(--accent);background:var(--surface-hover)}button:disabled{opacity:.5;cursor:not-allowed}
  .back,.refresh{border-color:transparent;background:transparent;padding:6px 0;min-height:32px;color:var(--text-dim)}.back:hover:not(:disabled),.refresh:hover:not(:disabled){background:transparent;border-color:transparent;color:var(--accent)}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  details{font-size:11px;color:var(--text-dim);line-height:1.8}summary{cursor:pointer;padding:4px 0}.testing{margin-top:12px}.testing p{margin:6px 0}.destination{margin:24px 0 16px}.section-heading{margin-bottom:12px}.section-heading>span{font-size:10px;color:var(--text-dim)}.empty{margin:12px 0 16px}
  .disks{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px}.disk{justify-content:flex-start;padding:18px;gap:14px;text-align:left;border-color:var(--border);background:var(--surface);min-height:84px}.disk.selected{background:var(--surface-raised);border-color:var(--accent);box-shadow:inset 0 0 0 1px var(--accent)}.disk.selected :global(svg){color:var(--accent)}.disk-copy{flex:1;display:flex;flex-direction:column;gap:6px}.disk-copy strong{font-size:13px}.disk-copy>span{font-size:10px;color:var(--text-dim)}
  .allocation,.blocked,article{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:22px;margin:16px 0}.space-source{display:block;font-size:11px;color:var(--text-muted);margin:0 0 16px}select{display:block;width:100%;margin-top:8px;padding:12px;background:var(--surface-raised);border:1px solid var(--border);color:var(--text);border-radius:4px;font-size:11px}.space-warning{border-left:2px solid var(--accent);padding:10px 14px;margin:14px 0;background:var(--surface-raised);font-size:11px}.space-warning p{margin:4px 0 0}.space-details{margin-bottom:18px}.continue{border-top:1px solid var(--border);padding-top:18px;display:flex;align-items:center;justify-content:space-between;gap:16px}.continue>span{font-size:10px;color:var(--text-dim)}.operation-actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:14px}.failure{border-left:2px solid var(--error);padding:12px 16px;font-size:12px;margin:16px 0}.failure p{overflow-wrap:anywhere}
  @media(max-width:550px){h2{font-size:20px}.allocation,.blocked,article{padding:16px}.continue{align-items:stretch;flex-direction:column;gap:10px}.section-heading{flex-wrap:wrap;gap:8px}.disks{grid-template-columns:1fr}}
</style>
