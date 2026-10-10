<script lang="ts">
  import { ArrowLeft, ArrowRight, HardDrive, RefreshCw } from 'lucide-svelte';
  import StagedDiskMap from './StagedDiskMap.svelte';
  import StagedPartitionTable from './StagedPartitionTable.svelte';
  import StagedReview from './StagedReview.svelte';
  import StagedSpaceEditor from './StagedSpaceEditor.svelte';
  import { setup, setupActive, type SetupSnapshot, type StagedBlockedDisk, type StagedChoice, type StagedDisk, type StagedOperation, type StagedRegion, type StagedResizeQuery } from './setup';
  import { choiceFor, defaultRegion, formatDiskSize, GiB, limits, overviewSegments, regionKey, regionStatus, type OverviewSegment, type RegionStatus } from './stagedLayout';
  export let recoveryOnly=false;
  export let close:()=>void=()=>{};
  export let sourceReady=false;
  export let prepareAgain:()=>void=()=>{};
  let selectedDisk='';
  // Map segment and space source the user picked; empty until they pick one,
  // so the default can follow the check as limits arrive.
  let chosenSegment='';
  let chosenSource='';
  let inspection:SetupSnapshot['stagedIso'];
  let restarting:string|null=null;
  let handledInspection='';
  let analyzing:StagedResizeQuery|null=null;
  let reading=false;
  let keepWindows=true;
  let omarchyGiB=64;
  let mapWidth=0;
  $: busy=$setup.pending || setupActive($setup.snapshot?.status);
  $: result=$setup.snapshot?.stagedIso;
  $: error=$setup.error ?? $setup.snapshot?.error;
  $: browsing=!busy || ($setup.snapshot?.status==='running' && ['inspecting','analyzing-resize'].includes($setup.snapshot?.stage) && !!result?.disks);
  $: if(!result && busy) handledInspection='';
  // The elevated check sends an initial layout, then updated resize limits.
  // Polling can return a new object every second; apply each version once.
  $: if(result?.disks && !error) {
    const signature=JSON.stringify({disks:result.disks,choices:result.choices,blocked:result.blocked,secureBoot:result.secureBoot});
    if(signature!==handledInspection) {
      handledInspection=signature;
      inspection=analyzing && !busy && inspection ? mergeInspection(inspection,result,analyzing) : result;
      analyzing=null;
    }
  } else if(result?.choices && !result.disks && !busy && !error) {
    const signature=JSON.stringify({choices:result.choices,blocked:result.blocked,secureBoot:result.secureBoot});
    if(signature!==handledInspection) { handledInspection=signature; inspection=result; }
  }
  $: if(error && analyzing) analyzing=null;
  $: disks=Array.from(new Map([
    ...(inspection?.choices ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:undefined as StagedBlockedDisk|undefined}] as const),
    ...(inspection?.disks ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:undefined as StagedBlockedDisk|undefined}] as const),
    ...(inspection?.blocked ?? []).map(c=>[c.diskUniqueId,{id:c.diskUniqueId,number:c.diskNumber,size:c.diskSizeBytes,blocked:c}] as const)
  ]).values()).sort((a,b)=>a.number-b.number);
  $: disk=disks.find(d=>d.id===selectedDisk);
  $: if(disks.length && !disks.some(d=>d.id===selectedDisk)) selectedDisk=disks.find(d=>!d.blocked)?.id ?? disks[0]?.id ?? '';
  $: layout=inspection?.disks?.find(d=>d.diskUniqueId===selectedDisk);
  $: if(inspection) reading=false;
  $: requiredBytes=inspection?.temporaryBytes ?? 0;
  $: installMinimum=inspection?.minimumLinuxBytes ?? 32*GiB;
  $: spaces=(inspection?.choices ?? []).filter(c=>c.diskUniqueId===selectedDisk && !disk?.blocked);
  $: segments=layout ? overviewSegments(layout,{barPx:mapWidth || 760}) : [];
  $: statuses=Object.fromEntries(segments.map(s=>[s.key,statusFor(s.regions,spaces,busy,analyzing,requiredBytes,installMinimum)])) as Record<string,RegionStatus>;
  $: autoRegion=layout ? defaultRegion(layout,spaces,requiredBytes,installMinimum) : undefined;
  $: selectedSegment=segments.find(s=>s.key===chosenSegment) ?? segments.find(s=>!!autoRegion && s.regions.includes(autoRegion)) ?? segments[0];
  $: sourceRegion=layout?.regions.find(r=>regionKey(r)===chosenSource && choiceFor(r,spaces)) ?? (autoRegion && choiceFor(autoRegion,spaces) ? autoRegion : undefined);
  // Without a layout (an older provider), use the first choice as before.
  $: choice=layout ? (sourceRegion ? choiceFor(sourceRegion,spaces) : undefined) : spaces[0];
  // Shrinking can also leave room for Omarchy after the temporary installer,
  // so Windows can be kept. Replacing Windows happens later in the installer.
  $: shrinkRegion=choice?.target.target_kind==='shrink' ? layout?.regions.find(r=>r.partitionGuid===choice?.target.partition_guid) : undefined;
  $: targetName=shrinkRegion?.isWindows===false ? shrinkRegion.label : 'Windows';
  $: ({minGiB:minimumGiB,maxGiB:keepMaximumGiB,canKeep}=limits(shrinkRegion,requiredBytes,installMinimum));
  $: if(canKeep && (omarchyGiB>keepMaximumGiB || omarchyGiB<minimumGiB)) omarchyGiB=Math.min(Math.max(omarchyGiB,minimumGiB),keepMaximumGiB);
  $: linuxBytes=canKeep && keepWindows ? omarchyGiB*GiB : 0;
  $: checkingAny=busy && (!!analyzing || !!layout?.regions.some(r=>r.resizeState==='unchecked'));
  $: tableStatus=(region:StagedRegion) => statusFor([region],spaces,busy,analyzing,requiredBytes,installMinimum).text;
  // The map and the selected-partition line show disk-check progress, so the
  // page-bottom status is hidden then; a hidden status region keeps it for
  // screen readers, and says when the check is over.
  $: quiet=!recoveryOnly && (inspection ? browsing || !!analyzing : reading);
  let checkStatus='';
  $: if(quiet && busy) checkStatus=$setup.snapshot?.message || 'Checking disks…';
  $: if(!busy && checkStatus) checkStatus=inspection ? 'Disk check finished.' : '';
  // A review waiting for an answer; once approved, the helper keeps it marked
  // approved until preparation ends, and a panel shows progress in its place.
  $: review=$setup.snapshot?.stagedReview?.approved ? null : $setup.snapshot?.stagedReview;
  let approving=false;
  $: if(!busy) approving=false;
  // Review and the preparing panel are part of the full flow; on the home
  // screen (recoveryOnly) the status, progress and Stop below stand in.
  $: preparing=!recoveryOnly && busy && (approving || !!$setup.snapshot?.stagedReview?.approved);
  function respond(approved:boolean) {
    if(approved) approving=true;
    void setup.respondStagedReview(approved);
  }
  $: copying=$setup.snapshot?.stage==='copying-installer' && ($setup.snapshot?.totalBytes ?? 0)>0;
  // Unknown until disks are checked; the provider reports when it is already off.
  $: secureBootMaybeOn=inspection?.secureBoot!==false;
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
  // Emphasize only the next step: turn Secure Boot off, select the installer,
  // or remove it once Windows has restarted after it was selected.
  function nextAction(op:StagedOperation) {
    if(op.status==='boot-scheduled') return op.restartedSinceScheduled ? 'cleanup' : null;
    if(['staged','arming'].includes(op.status)) return inspection?.secureBoot ? 'firmware' : 'arm';
    return null;
  }
  function heading(op:StagedOperation) {
    if(op.status==='boot-scheduled') return op.restartedSinceScheduled ? 'Windows restarted after selecting the installer' : 'Installer selected for a restart';
    return ['staged','arming'].includes(op.status) ? 'Temporary installer ready' : 'Installer preparation needs attention';
  }
  function inspect() {
    inspection=undefined; analyzing=null; reading=true;
    void setup.stagedIso('inspect');
  }
  // A partition being rechecked shows as checking until its new limit arrives.
  function statusFor(regions:StagedRegion[],choices:StagedChoice[],checking:boolean,query:StagedResizeQuery|null,temporaryBytes:number,minimumBytes:number) {
    const rechecking=(r:StagedRegion) => checking && !!query && r.partitionGuid===query.partitionGuid;
    const shown=regions.map(r=>rechecking(r) ? {...r,resizeState:'unchecked' as const} : r);
    const region=shown[0];
    return regionStatus(shown.length>1 ? shown : region,{temporaryBytes,minimumBytes,usable:!rechecking(regions[0]) && !!choiceFor(region,choices),checking:checking && region.resizeState==='unchecked'});
  }
  // Any pick fixes the source: a usable segment becomes it, and any other
  // keeps the current one, even if the default would move later.
  function selectSegment(key:string) {
    const segment=segments.find(s=>s.key===key);
    const source=segment?.regions.length===1 && choiceFor(segment.regions[0],spaces) ? segment.regions[0] : sourceRegion;
    chosenSegment=key; chosenSource=source ? regionKey(source) : '';
  }
  // Editing the space fixes the source shown in the editor, even when it
  // stands in for an earlier pick that can't be used yet, so a measurement
  // can't move the space being edited. A selection the user hasn't made
  // yet is fixed too.
  function pin() {
    if(!chosenSegment && selectedSegment) chosenSegment=selectedSegment.key;
    if(sourceRegion) chosenSource=regionKey(sourceRegion);
  }
  function selectDisk(id:string) {
    selectedDisk=id; chosenSegment=''; chosenSource='';
  }
  function diskName(item:{number:number;size?:number;blocked?:StagedBlockedDisk}) {
    return `Disk ${item.number}${item.size ? ` · ${formatDiskSize(item.size)}` : ''}${item.blocked ? " · can't use" : ''}`;
  }
  const single=(segment:OverviewSegment|undefined) => segment?.regions.length===1 ? segment.regions[0] : undefined;
  function analyze(region:StagedRegion) {
    if(!disk || !layout || busy || !testing || !sourceReady || !region.partitionNumber || !region.partitionGuid) return;
    const query={diskNumber:disk.number,diskUniqueId:disk.id,partitionNumber:region.partitionNumber,partitionGuid:region.partitionGuid};
    analyzing=query;
    void setup.stagedIso('inspect',null,null,query);
  }
  async function stage() {
    if(!testing || !sourceReady || !choice || busy || error) return;
    const selection:Record<string,unknown>={diskNumber:choice.diskNumber,diskUniqueId:choice.diskUniqueId,target:choice.target};
    if(linuxBytes>0) selection.linuxBytes=linuxBytes;
    await setup.stagedIso('stage',selection);
  }
</script>

{#if !recoveryOnly || operations.length || recordErrors.length}
<section class="staged" aria-label="Temporary installer">
  {#if !recoveryOnly}
    <div class="navigation"><button class="back" disabled={busy} onclick={close}><ArrowLeft size={15}/>Back</button>{#if testing}<span class="badge">Test build</span>{/if}</div>
    <h2>Install without USB</h2>
    <p class="intro">Make room for Omarchy on this computer.</p>
    <p class="visually-hidden" role="status">{checkStatus}</p>
    {#if review}
      <StagedReview summary={review.summary} plan={review.review} disk={inspection?.disks?.find(d=>d.diskNumber===review?.review?.diskNumber)} secureBoot={inspection?.secureBoot} pending={$setup.pending} {respond}/>
    {:else if preparing}
      <div class="preparing" role="region" aria-label="Preparing the installer">
        <h3>Preparing the installer</h3>
        <p role="status">{$setup.snapshot?.message || 'Preparing…'}</p>
        {#if copying}<progress value={$setup.snapshot?.bytes ?? 0} max={$setup.snapshot?.totalBytes ?? 0} aria-label="Copying the installer"></progress>{/if}
        {#if $setup.snapshot?.cancelAvailable}<button disabled={$setup.pending || $setup.snapshot?.cancelRequested} onclick={()=>{void setup.cancel();}}>Stop preparing</button>{/if}
      </div>
    {:else if !inspection}
      {#if reading && busy}
        <div class="map-placeholder" aria-hidden="true"></div>
        <p class="line">Reading disks…</p>
      {:else}
        <div class="start">
          <button class="primary" disabled={busy || !testing || !sourceReady} onclick={inspect}><HardDrive size={16}/>Check available space</button>
          <p>{sourceReady ? 'Windows will ask for administrator access. Nothing is changed.' : 'Download the image above first.'}</p>
        </div>
      {/if}
    {:else}
      <div class="disk-row" data-measure-start>
        {#if !disks.length}<p class="empty">No internal disks found. Check Disk Management, then refresh.</p>
        {:else if disks.length===1}<h3>{diskName(disks[0])}</h3>
        {:else}<div class="disks" role="group" aria-label="Disks">{#each disks as item (item.id)}<button class="disk" class:selected={selectedDisk===item.id} aria-pressed={selectedDisk===item.id} disabled={!sourceReady || !browsing} onclick={()=>selectDisk(item.id)}>{diskName(item)}</button>{/each}</div>{/if}
        <button class="refresh" disabled={busy || !sourceReady || !testing} onclick={inspect}><RefreshCw size={14}/>Refresh</button>
      </div>
      {#if disk?.blocked}
        <p class="blocked">{disk.blocked.reason}</p>
      {:else if layout}
        <StagedDiskMap label={`Partitions on Disk ${layout.diskNumber}`} {segments} {statuses} selected={selectedSegment?.key ?? ''} source={sourceRegion ? regionKey(sourceRegion) : ''} select={selectSegment} disabled={!sourceReady || !(browsing || analyzing)} bind:width={mapWidth}/>
        {#if selectedSegment}
          {@const status=statuses[selectedSegment.key]}
          {@const region=single(selectedSegment)}
          <p class="line">{status?.text}
            {#if status?.action==='recheck' && region}<button class="text-action" aria-label={`Recheck ${region.label}`} disabled={busy || !testing || !sourceReady} onclick={()=>analyze(region)}>Recheck</button>
            {:else if status?.action==='refresh'}<button class="text-action" aria-label="Refresh disks" disabled={busy || !testing || !sourceReady} onclick={inspect}>Refresh</button>{/if}
          </p>
        {/if}
      {/if}
      {#if !disk?.blocked && (layout || choice)}
        {#if choice}
          <div class="allocation">
            {#if layout && sourceRegion && single(selectedSegment)!==sourceRegion}<p>Space comes from {sourceRegion.kind==='free' ? 'free space' : sourceRegion.label}.</p>{/if}
            {#if !layout}<p>{choice.label}</p>{/if}
            {#key sourceRegion ? regionKey(sourceRegion) : JSON.stringify(choice.target)}
              <StagedSpaceEditor {choice} region={sourceRegion} {targetName} temporaryBytes={requiredBytes} minimumBytes={installMinimum} minGiB={minimumGiB} maxGiB={keepMaximumGiB} {canKeep} bind:keep={keepWindows} bind:omarchyGiB disabled={!browsing} edit={pin}/>
            {/key}
          </div>
        {:else if layout && !checkingAny}<p class="no-space">No space can be used yet.</p>{/if}
        <div class="continue">
          {#if layout}<StagedPartitionTable regions={layout.regions} statusText={tableStatus} unallocatedBytes={layout.unallocatedBytes}/>{/if}
          {#if choice}<button class="primary submit" disabled={busy || !!error || !testing || !sourceReady} onclick={stage}>Review changes<ArrowRight size={16}/></button>{/if}
        </div>
      {/if}
    {/if}
  {/if}
  {#each recordErrors as record}
    <p role="alert">A previous temporary installer could not be identified safely. Its partitions have not been changed.</p>
    <details><summary>Details</summary><p>{record.operationId}: {record.message}</p></details>
  {/each}
  {#each operations as op}
    <article>
      <h3>{heading(op)}</h3>
      <p>Disk {op.diskNumber} · {(op.temporaryBytes/1024**3).toFixed(1)} GiB temporary storage</p>
      {#if op.status==='boot-scheduled'}<p>{op.restartedSinceScheduled ? 'If Omarchy is installed and starts on its own, resume BitLocker when reminded, then remove the temporary installer. To try again, select the installer for the next restart.' : 'Restart Windows to start the installer.'}</p>{/if}
      {#if ['staged','arming','boot-scheduled'].includes(op.status)}
        <details class="next-steps" open={!recoveryOnly}>
          <summary>What happens next</summary>
          <ol>
            <li>If Secure Boot is on, turn it off first with Restart to firmware settings.</li>
            <li>Select the installer for the next restart, then restart Windows.</li>
            <li>In the installer, choose the disk marked “installer disk, free space only”, then Free space install. If space is short, it lets you delete a partition you no longer need, such as Windows.</li>
            <li>Omarchy starts by default afterwards. To open Windows, use your computer’s boot menu key (often F12, F11 or Esc), or run limine-scan in Omarchy once to add Windows to its menu.</li>
            <li>In Windows, resume BitLocker when reminded, then remove the temporary installer here. If you replaced Windows, the temporary partitions and firmware entry stay on the disk.</li>
          </ol>
        </details>
      {/if}
      <details><summary>When should I remove it?</summary><p>Remove it after Omarchy boots independently, or to abandon this attempt. Windows and installed Omarchy are kept. These actions require administrator approval.</p></details>
      <div class="operation-actions">{#if testing && ['staged','arming','boot-scheduled'].includes(op.status)}{#if secureBootMaybeOn && nextAction(op)==='firmware'}<button class="primary" disabled={busy} onclick={()=>{void setup.stagedIso('firmware',null,op.operationId);}}>Restart to firmware settings</button>{/if}<button class:primary={nextAction(op)==='arm'} disabled={busy} onclick={()=>{void setup.stagedIso('arm',null,op.operationId);}}>{op.status==='boot-scheduled' ? 'Start installer again on next restart' : 'Start installer on next restart'}</button>{#if secureBootMaybeOn && nextAction(op)!=='firmware'}<button disabled={busy} onclick={()=>{void setup.stagedIso('firmware',null,op.operationId);}}>Restart to firmware settings</button>{/if}{/if}
      <button class:primary={nextAction(op)==='cleanup'} disabled={busy} onclick={()=>{void setup.stagedIso('cleanup',null,op.operationId);}}>Remove temporary installer</button>
      {#if testing && sourceReady}<button disabled={busy} onclick={()=>{restarting=op.operationId;void setup.stagedIso('cleanup',null,op.operationId);}}>Prepare again</button>{/if}</div>
    </article>
  {/each}
  {#if busy && !quiet && !(review && !recoveryOnly) && !preparing}<p role="status">{$setup.snapshot?.message || 'Checking…'}</p>{/if}
  {#if busy && copying && !preparing}<progress value={$setup.snapshot?.bytes ?? 0} max={$setup.snapshot?.totalBytes ?? 0} aria-label="Copying the installer"></progress>{/if}
  {#if busy && $setup.snapshot?.cancelAvailable && !preparing}<button disabled={$setup.pending || $setup.snapshot?.cancelRequested} onclick={()=>{void setup.cancel();}}>Stop preparing</button>{/if}
  {#if !busy && result?.message}<p role="status">{result.message}</p>{/if}
  {#if !busy && $setup.snapshot?.bitLocker}<p role="status">{$setup.snapshot.bitLocker.message}</p>{/if}
  {#if error}<div class="failure" role="alert"><strong>Couldn’t complete this step.</strong><details><summary>Show details</summary><p>{error}</p></details></div>{/if}
</section>
{/if}

<style>
  .staged{margin-top:22px}h2{font-size:22px;letter-spacing:-.6px;margin:18px 0 8px}h3{font-size:13px;margin:0}p{font-size:11px;line-height:1.8;color:var(--text-muted)}.intro{margin:0;font-size:12px}
  .navigation{display:flex;align-items:center;justify-content:space-between;gap:16px}.badge{font-size:10px;color:var(--accent);background:var(--surface-raised);border:1px solid var(--border);border-radius:20px;padding:5px 10px}
  button{display:inline-flex;align-items:center;justify-content:center;gap:9px;min-height:42px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:hover:not(:disabled){border-color:var(--accent);background:var(--surface-hover)}button:disabled{opacity:.5;cursor:not-allowed}
  .back,.refresh{border-color:transparent;background:transparent;padding:6px 0;min-height:32px;color:var(--text-dim)}.back:hover:not(:disabled),.refresh:hover:not(:disabled){background:transparent;border-color:transparent;color:var(--accent)}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  details{font-size:11px;color:var(--text-dim);line-height:1.8}summary{cursor:pointer;padding:4px 0}
  .start{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;margin:24px 0 16px}.start p{margin:0;font-size:12px}
  .disk-row{display:flex;align-items:center;justify-content:space-between;gap:8px 16px;min-height:32px;margin-top:24px}.empty{margin:0;font-size:12px}
  .disks{display:flex;flex-wrap:wrap;gap:6px}.disk{min-height:32px;padding:6px 12px;border-color:var(--border);background:var(--surface);color:var(--text-muted);font-size:12px}.disk.selected{border-color:var(--accent);background:var(--surface-raised);color:var(--text)}
  .map-placeholder{height:40px;margin:24px 0 10px;border-radius:3px;background:linear-gradient(90deg,var(--surface-raised) 0%,var(--surface-hover) 50%,var(--surface-raised) 100%) 0 0/200% 100%;animation:shimmer 1.6s linear infinite}
  .line{margin:0;font-size:12px;color:var(--text)}.text-action{min-height:0;padding:0 0 0 6px;border:0;background:transparent;color:var(--accent);font-size:12px;text-decoration:underline;text-underline-offset:3px}.text-action:hover:not(:disabled){border-color:transparent;background:transparent;color:var(--accent-hover)}
  .blocked{margin:14px 0 0;font-size:12px}
  .allocation{margin-top:18px}.allocation p,.no-space{margin:0 0 8px;font-size:12px}.no-space{margin-top:18px;color:var(--text)}
  .visually-hidden{position:absolute;width:1px;height:1px;margin:-1px;padding:0;border:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
  .continue{display:grid;grid-template-columns:1fr auto;grid-template-areas:"toggle action" "table table";align-items:center;gap:0 16px;border-top:1px solid var(--border);margin-top:16px;padding-top:14px}.submit{grid-area:action;justify-self:end}
  .preparing,article{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:22px;margin:16px 0}.preparing{margin-top:24px;border-color:var(--accent)}.preparing h3{margin-bottom:6px}.preparing p{margin:0 0 8px;font-size:12px}
  .operation-actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:14px}.failure{border-left:2px solid var(--error);padding:12px 16px;font-size:12px;margin:16px 0}.failure p{overflow-wrap:anywhere}
  .next-steps ol{margin:6px 0 0;padding-left:18px;font-size:11px;line-height:1.8;color:var(--text-muted)}
  progress{width:100%;height:6px;margin:4px 0 12px;accent-color:var(--accent)}
  @keyframes shimmer{from{background-position:100% 0}to{background-position:-100% 0}}
  @media(prefers-reduced-motion:reduce){.map-placeholder{animation:none}}
  @media(max-width:550px){h2{font-size:20px}.preparing,article{padding:16px}.continue{grid-template-columns:1fr;grid-template-areas:"action" "toggle" "table";gap:10px}.submit{justify-self:stretch}}
</style>
