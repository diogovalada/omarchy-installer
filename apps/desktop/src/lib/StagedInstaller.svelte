<script lang="ts">
  import { ArrowLeft, ArrowRight, HardDrive, RefreshCw } from 'lucide-svelte';
  import StagedDiskMap from './StagedDiskMap.svelte';
  import StagedPartitionTable from './StagedPartitionTable.svelte';
  import StagedReview from './StagedReview.svelte';
  import StagedSteps from './StagedSteps.svelte';
  import StagedSpaceEditor from './StagedSpaceEditor.svelte';
  import { setup, setupActive, type SetupSnapshot, type StagedBlockedDisk, type StagedChoice, type StagedDisk, type StagedOperation, type StagedRegion, type StagedResizeQuery } from './setup';
  import { choiceFor, defaultRegion, formatDiskSize, GiB, limits, overviewSegments, regionKey, regionStatus, type OverviewSegment, type RegionStatus } from './stagedLayout';
  export let recoveryOnly=false;
  export let close:()=>void=()=>{};
  export let sourceReady=false;
  // The chosen image's identity, known before it finishes downloading; a disk
  // check needs only that, Review needs the downloaded image.
  export let image='';
  $: imageKnown=sourceReady || !!image;
  export let prepareAgain:()=>void=()=>{};
  // Opens the USB installer instead, when no space can be used here.
  export let useUsb:(()=>void)|null=null;
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
  // The check started in the background at launch. Its results stand in
  // until the user runs a check of their own; while it runs, partitions it
  // hasn't measured show as being checked, and any action stops it first.
  // Results for another image don't count: its size sets the space needed.
  $: check=!image || $setup.snapshot?.stagedCheck?.identity===image ? $setup.snapshot?.stagedCheck : null;
  $: background=check?.status==='running' || check?.status==='stopping';
  $: checking=busy || background;
  // A check run by hand counts only for the image it ran for, recorded on
  // its result, so it holds after Review and after reopening the screen.
  $: own=busy || !image || !result?.disks || result.image===image ? result : null;
  $: scan=own?.disks || busy ? own : (check?.result ?? own);
  // Choosing another image drops what is shown: its size sets the space needed.
  let shownImage=image;
  $: if(image!==shownImage) { shownImage=image; inspection=undefined; handledInspection=''; heldDefault=''; analyzing=null; chosenSegment=''; chosenSource=''; }
  $: browsing=!busy || ($setup.snapshot?.status==='running' && ['inspecting','analyzing-resize'].includes($setup.snapshot?.stage) && !!result?.disks);
  $: if(!result && busy) handledInspection='';
  // The elevated check sends an initial layout, then updated resize limits.
  // Polling can return a new object every second; apply each version once.
  $: if(scan?.disks && !error) {
    const signature=JSON.stringify({disks:scan.disks,choices:scan.choices,blocked:scan.blocked,secureBoot:scan.secureBoot});
    if(signature!==handledInspection) {
      handledInspection=signature;
      inspection=analyzing && !busy && inspection ? mergeInspection(inspection,scan,analyzing) : scan;
      analyzing=null;
    }
  } else if(scan?.choices && !scan.disks && !busy && !error) {
    const signature=JSON.stringify({choices:scan.choices,blocked:scan.blocked,secureBoot:scan.secureBoot});
    if(signature!==handledInspection) { handledInspection=signature; inspection=scan; }
  }
  // A background check stopped for another action picks up again here.
  let resumed=false;
  $: if(!recoveryOnly && !resumed && check?.status==='stopped' && !busy && !own?.disks) { resumed=true; void setup.backgroundCheck(); }
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
  $: statuses=Object.fromEntries(segments.map(s=>[s.key,statusFor(s.regions,spaces,checking,analyzing,requiredBytes,installMinimum)])) as Record<string,RegionStatus>;
  $: autoRegion=layout ? defaultRegion(layout,spaces,requiredBytes,installMinimum) : undefined;
  // Once a usable default is shown, it stays while it is usable, so a later
  // measurement doesn't move the selection; measurements take minutes.
  let heldDefault='';
  $: if(autoRegion && choiceFor(autoRegion,spaces) && !layout?.regions.some(r=>regionKey(r)===heldDefault && choiceFor(r,spaces))) heldDefault=regionKey(autoRegion);
  $: shownDefault=layout?.regions.find(r=>regionKey(r)===heldDefault && choiceFor(r,spaces)) ?? autoRegion;
  $: selectedSegment=segments.find(s=>s.key===chosenSegment) ?? segments.find(s=>!!shownDefault && s.regions.includes(shownDefault)) ?? segments[0];
  $: sourceRegion=layout?.regions.find(r=>regionKey(r)===chosenSource && choiceFor(r,spaces)) ?? (shownDefault && choiceFor(shownDefault,spaces) ? shownDefault : undefined);
  $: usableCount=layout ? layout.regions.filter(r=>choiceFor(r,spaces)).length : 0;
  // With no usable space, help fits what blocks the default partition
  // (usually Windows): files it can't move, or a failed measurement.
  $: blockCause=!choice && shownDefault ? statusFor([shownDefault],spaces,checking,analyzing,requiredBytes,installMinimum).cause : undefined;
  $: blockName=shownDefault?.label ?? '';
  // Review waits for the disk check; say which partition it waits for.
  $: waitingFor=checking && browsing ? (inspection?.disks ?? []).flatMap(d=>d.regions).find(r=>analyzing ? r.partitionGuid===analyzing.partitionGuid : r.kind==='partition' && r.resizeState==='unchecked')?.label ?? '' : '';
  // Without a layout (an older provider), use the first choice as before.
  $: choice=layout ? (sourceRegion ? choiceFor(sourceRegion,spaces) : undefined) : spaces[0];
  // Shrinking can also leave room for Omarchy after the temporary installer,
  // so Windows can be kept. Replacing Windows happens later in the installer.
  $: shrinkRegion=choice?.target.target_kind==='shrink' ? layout?.regions.find(r=>r.partitionGuid===choice?.target.partition_guid) : undefined;
  $: targetName=shrinkRegion?.isWindows===false ? shrinkRegion.label : 'Windows';
  $: ({minGiB:minimumGiB,maxGiB:keepMaximumGiB,canKeep}=limits(shrinkRegion,requiredBytes,installMinimum));
  $: if(canKeep && (omarchyGiB>keepMaximumGiB || omarchyGiB<minimumGiB)) omarchyGiB=Math.min(Math.max(omarchyGiB,minimumGiB),keepMaximumGiB);
  $: linuxBytes=canKeep && keepWindows ? omarchyGiB*GiB : 0;
  $: checkingAny=checking && (!!analyzing || !!layout?.regions.some(r=>r.resizeState==='unchecked'));
  $: tableStatus=(region:StagedRegion) => {
    const status=statusFor([region],spaces,checking,analyzing,requiredBytes,installMinimum);
    return status.detail ? `${status.text} ${status.detail}` : status.text;
  };
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
  // A recorded temporary installer's steps replace the choose screen, whose
  // layout is then out of date. Checking disks again brings it back below
  // them, for when the installer can't be removed; preparing another one
  // hides it again.
  let checkingBeside=false;
  $: if(preparing) checkingBeside=false;
  function checkBeside() {
    checkingBeside=true;
    inspect();
  }
  // Work counted in bytes (copying, or checking the installer files before
  // restarting into it) shows a bar.
  $: measured=($setup.snapshot?.totalBytes ?? 0)>0;
  // Preparing's steps, from the provider: finished ones ticked, the current
  // one with its bar (a percentage when counted in bytes).
  $: steps=$setup.snapshot?.steps ?? [];
  $: stepIndex=steps.findIndex(s=>s.id===$setup.snapshot?.step);
  $: percent=measured ? Math.floor(100*($setup.snapshot?.bytes ?? 0)/($setup.snapshot?.totalBytes ?? 1)) : null;
  $: operations=($setup.snapshot?.stagedRecovery?.operations ?? result?.operations ?? []).filter(op=>op.status!=='cleaned' && !(result?.operationId===op.operationId && result.status==='cleaned'));
  $: recordErrors=$setup.snapshot?.stagedRecovery?.recordErrors ?? result?.recordErrors ?? [];
  $: testing=$setup.snapshot?.stagedTesting===true;
  // The snapshot names the staged action it reflects, even after reloading.
  $: lastAction=$setup.snapshot?.stagedTarget?.action ?? '';
  // Declining any confirmation of a staged action changes nothing; the helper
  // reports it as cancelled without an error. Review's shows by its button.
  $: cancelledNote=!busy && !error && $setup.snapshot?.status==='cancelled' ? $setup.snapshot.message : '';
  $: declined=lastAction==='stage' ? cancelledNote : '';
  // The steps card says what to do after preparing or setting the next
  // restart, so only a firmware restart's or a removal's own result shows;
  // a removed installer's card is gone.
  $: resultNote=!busy && $setup.snapshot?.status==='complete' && ['firmware','cleanup'].includes(lastAction) ? result?.message ?? '' : lastAction!=='stage' ? cancelledNote : '';
  // The recorded installer a running command changes: the one its Review
  // prepares, or the one being removed. Others keep their own state, and a
  // disk check changes none.
  $: preparingId=busy ? $setup.snapshot?.stagedReview?.review?.operationId ?? null : null;
  // The target describes the snapshot's own operation, so it counts only while
  // that runs (not while the next command is pending).
  // A removal is under way once its confirmation is approved.
  $: removingId=setupActive($setup.snapshot?.status) && $setup.snapshot?.stagedTarget?.action==='cleanup' && $setup.snapshot.stagedTarget.confirmed ? $setup.snapshot.stagedTarget.operationId : null;
  $: underway=(op:StagedOperation) => op.operationId===preparingId ? 'preparing' as const : op.operationId===removingId ? 'removing' as const : null;
  $: if(restarting && !busy && result?.operationId===restarting && result.status==='cleaned') {
    restarting=null; selectedDisk=''; prepareAgain(); inspect();
  }
  // A failed or declined removal ends Prepare again.
  $: if(error || (!busy && $setup.snapshot?.status==='cancelled')) restarting=null;
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
        return saved?.resizeState==='checked' && saved.freeBytes===region.freeBytes ? {...region,resizeState:saved.resizeState,resizeReason:saved.resizeReason,maximumReleaseBytes:saved.maximumReleaseBytes,reserveBytes:saved.reserveBytes,limitedByFiles:saved.limitedByFiles,filesReleaseBytes:saved.filesReleaseBytes} : region;
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
  // Recorded installer actions; Prepare again removes it, then checks disks.
  // Checking disks beside an installer is offered once removing it failed:
  // the last command was a removal and the operation itself ended as failed
  // (not a polling or other error). After other failures, retrying is what
  // helps. The setup store outlives this screen, so the offer carries from
  // the home screen into the flow.
  $: removalFailed=operations.length>0 && lastAction==='cleanup' && $setup.snapshot?.status==='failed';
  function operate(op:StagedOperation,action:'restart'|'arm'|'firmware'|'cleanup'|'again') {
    if(action==='again') restarting=op.operationId;
    void setup.stagedIso(action==='again' ? 'cleanup' : action,null,op.operationId);
  }
  function inspect() {
    inspection=undefined; analyzing=null; reading=true; heldDefault='';
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
    selectedDisk=id; chosenSegment=''; chosenSource=''; heldDefault='';
  }
  function diskName(item:{number:number;size?:number;blocked?:StagedBlockedDisk}) {
    return `Disk ${item.number}${item.size ? ` · ${formatDiskSize(item.size)}` : ''}${item.blocked ? " · can't use" : ''}`;
  }
  const single=(segment:OverviewSegment|undefined) => segment?.regions.length===1 ? segment.regions[0] : undefined;
  function analyze(region:StagedRegion) {
    if(!disk || !layout || busy || !testing || !imageKnown || !region.partitionNumber || !region.partitionGuid) return;
    const query={diskNumber:disk.number,diskUniqueId:disk.id,partitionNumber:region.partitionNumber,partitionGuid:region.partitionGuid};
    analyzing=query;
    void setup.stagedIso('inspect',null,null,query);
  }
  // Review takes a while after Windows asks for access; say so beside the
  // button rather than at the page bottom.
  let requesting=false;
  $: if(review || !busy) requesting=false;
  // Beside Review while it is planned: the planning step's own message
  // (measuring the partition, which is the long part), else a plain line
  // rather than the helper's technical stages.
  $: planningMessage=busy && $setup.snapshot?.stage==='planning' && $setup.snapshot.message ? $setup.snapshot.message : 'Planning the changes…';
  // A stopped check finishes the partition Windows is measuring first.
  $: stopLabel=quiet ? ($setup.snapshot?.cancelRequested ? `Stopping after ${waitingFor || 'the current partition'}…` : 'Stop checking') : 'Stop preparing';
  // Planning a Review, from the snapshot, so it holds after reloading.
  $: planning=busy && $setup.snapshot?.stagedTarget?.action==='stage' && !$setup.snapshot?.stagedReview;
  async function stage() {
    if(!testing || !sourceReady || !choice || busy || error) return;
    requesting=true;
    const selection:Record<string,unknown>={diskNumber:choice.diskNumber,diskUniqueId:choice.diskUniqueId,target:choice.target};
    if(linuxBytes>0) selection.linuxBytes=linuxBytes;
    await setup.stagedIso('stage',selection);
  }
</script>

{#if !recoveryOnly || operations.length || recordErrors.length || resultNote}
<section class="staged" aria-label="Temporary installer">
  {#if !recoveryOnly}
    <div class="navigation"><button class="back" disabled={busy} onclick={close}><ArrowLeft size={15}/>Back</button>{#if testing}<span class="badge">Test build</span>{/if}</div>
    <h2>Install without USB</h2>
    <p class="intro">Make room for Omarchy on this computer.</p>
    {#if $setup.snapshot?.admin==='declined' && !busy}<p class="admin">Administrator access lets the installer check your disks in the background. <button class="text-action" onclick={()=>{void setup.startAdminSession();}}>Allow</button></p>{/if}
    <p class="visually-hidden" role="status">{checkStatus}</p>
    {#if review}
      <StagedReview summary={review.summary} plan={review.review} disk={inspection?.disks?.find(d=>d.diskNumber===review?.review?.diskNumber)} secureBoot={inspection?.secureBoot} pending={$setup.pending} {respond}/>
    {:else if preparing}
      <div class="preparing" role="region" aria-label="Preparing the installer">
        <h3>Preparing the installer</h3>
        {#if steps.length}
          <ol class="progress-steps" aria-label="Steps">
            {#each steps as item,index (item.id)}
              <li class:done={index<stepIndex} class:current={index===stepIndex} aria-current={index===stepIndex ? 'step' : undefined}>
                <span>{item.label}{index===stepIndex && percent!=null ? ` · ${percent}%` : ''}</span>
                {#if index===stepIndex}
                  {#if measured}<progress value={$setup.snapshot?.bytes ?? 0} max={$setup.snapshot?.totalBytes ?? 0} aria-label={item.label}></progress>
                  {:else}<progress aria-label={item.label}></progress>{/if}
                  {#if item.id==='shrink'}<small>This can take several minutes.</small>{/if}
                {/if}
              </li>
            {/each}
          </ol>
          <p class="visually-hidden" role="status">{$setup.snapshot?.message ?? ''}</p>
        {:else}
          <p role="status">{$setup.snapshot?.message || 'Preparing…'}</p>
          {#if measured}<progress value={$setup.snapshot?.bytes ?? 0} max={$setup.snapshot?.totalBytes ?? 0} aria-label="Preparing the installer"></progress>{/if}
        {/if}
        <p>Keep this window open.</p>
        {#if $setup.snapshot?.cancelAvailable}<button disabled={$setup.pending || $setup.snapshot?.cancelRequested} onclick={()=>{void setup.cancel();}}>Stop preparing</button>{/if}
      </div>
    {:else if operations.length && !checkingBeside}
      <!-- The recorded installer's steps below stand in for the choose screen. -->
    {:else if !inspection}
      {#if (reading && busy) || (background && !check?.result)}
        <div class="map-placeholder" aria-hidden="true"></div>
        <p class="line">Reading disks…</p>
      {:else}
        <div class="start">
          <button class="primary" disabled={busy || !testing || !imageKnown} onclick={inspect}><HardDrive size={16}/>Check available space</button>
          <p>{!imageKnown ? 'Download the image above first.' : $setup.snapshot?.admin==='ready' ? 'Nothing is changed.' : 'Windows will ask for administrator access. Nothing is changed.'}</p>
        </div>
      {/if}
    {:else}
      <div class="disk-row" data-measure-start>
        {#if !disks.length}<p class="empty">No internal disks found. Check Disk Management, then refresh.</p>
        {:else if disks.length===1}<h3>{diskName(disks[0])}</h3>
        {:else}<div class="disks" role="group" aria-label="Disks">{#each disks as item (item.id)}<button class="disk" class:selected={selectedDisk===item.id} aria-pressed={selectedDisk===item.id} disabled={!browsing} onclick={()=>selectDisk(item.id)}>{diskName(item)}</button>{/each}</div>{/if}
        {#if usableCount>1 && !disk?.blocked}<span class="hint">Select where the space comes from.</span>{/if}
        <button class="refresh" disabled={busy || !imageKnown || !testing} onclick={inspect}><RefreshCw size={14}/>Refresh</button>
      </div>
      {#if disk?.blocked}
        <p class="blocked">{disk.blocked.reason}</p>
      {:else if layout}
        <StagedDiskMap label={`Partitions on Disk ${layout.diskNumber}`} {segments} {statuses} selected={selectedSegment?.key ?? ''} source={sourceRegion ? regionKey(sourceRegion) : ''} select={selectSegment} disabled={!(browsing || analyzing)} bind:width={mapWidth}/>
        {#if selectedSegment}
          {@const status=statuses[selectedSegment.key]}
          {@const region=single(selectedSegment)}
          <p class="line">{status?.text}
            {#if sourceRegion && region!==sourceRegion}<span class="source-note">Space comes from {sourceRegion.kind==='free' ? 'free space' : sourceRegion.label}.</span>{/if}
            {#if status?.action==='recheck' && region}<button class="text-action" aria-label={`Recheck ${region.label}`} disabled={busy || !testing || !imageKnown} onclick={()=>analyze(region)}>Recheck</button>
            {:else if status?.action==='refresh'}<button class="text-action" aria-label="Refresh disks" disabled={busy || !testing || !imageKnown} onclick={inspect}>Refresh</button>{/if}
          </p>
        {/if}
      {/if}
      {#if !disk?.blocked && (layout || choice)}
        {#if choice}
          <div class="allocation">
            {#if !layout}<p>{choice.label}</p>{/if}
            {#key sourceRegion ? regionKey(sourceRegion) : JSON.stringify(choice.target)}
              <StagedSpaceEditor {choice} region={sourceRegion} {targetName} temporaryBytes={requiredBytes} minimumBytes={installMinimum} minGiB={minimumGiB} maxGiB={keepMaximumGiB} {canKeep} bind:keep={keepWindows} bind:omarchyGiB disabled={!browsing} edit={pin}/>
            {/key}
          </div>
        {:else if layout && !checkingAny}
          <p class="no-space">No space can be used yet.</p>
          {#if blockCause==='files' || blockCause==='measure' || useUsb}
            <details class="help">
              <summary>What can help?</summary>
              {#if blockCause==='files'}<p>Turning off hibernation and System Restore, then restarting, often lets Windows shrink {blockName} further.</p>
              {:else if blockCause==='measure'}<p>Restart Windows, then recheck.</p>{/if}
              {#if useUsb}<p>{#if blockCause==='files' || blockCause==='measure'}Or <button class="text-action inline" disabled={busy} onclick={useUsb}>create a bootable USB</button>{:else}<button class="text-action inline" disabled={busy} onclick={useUsb}>Create a bootable USB</button>{/if} and replace Windows.</p>{/if}
            </details>
          {/if}
        {/if}
        <div class="continue">
          {#if layout}<StagedPartitionTable regions={layout.regions} statusText={tableStatus} unallocatedBytes={layout.unallocatedBytes}/>{/if}
          {#if choice}<div class="submit">{#if waitingFor}<span class="waiting">Still checking {waitingFor}…</span>{:else if requesting}<span class="waiting" role="status">{planningMessage}</span>{:else if declined}<span class="waiting" role="status">{declined}</span>{:else if !sourceReady}<span class="waiting">Download the image above first.</span>{/if}<button class="primary" disabled={busy || !!error || !testing || !sourceReady} onclick={stage}>Review changes<ArrowRight size={16}/></button></div>{/if}
        </div>
      {/if}
    {/if}
  {/if}
  {#each recordErrors as record}
    <p role="alert">A previous temporary installer could not be identified safely. Its partitions have not been changed.</p>
    <details><summary>Details</summary><p>{record.operationId}: {record.message}</p></details>
  {/each}
  {#each operations as op (op.operationId)}
    <!-- While preparing, the panel above stands in for the new installer's card. -->
    {#if !(preparing && op.operationId===preparingId)}
      <StagedSteps {op} secureBoot={$setup.secureBoot ?? inspection?.secureBoot} {testing} canPrepareAgain={testing && sourceReady} {busy} underway={underway(op)} act={action=>operate(op,action)}/>
    {/if}
  {/each}
  {#if !recoveryOnly && removalFailed && !checkingBeside && !review && !preparing}<p class="beside"><button class="text-action" disabled={busy || !testing || !sourceReady} onclick={checkBeside}>Check disks again</button></p>{/if}
  {#if busy && !quiet && !(review && !recoveryOnly) && !preparing && !requesting}<p role="status">{$setup.snapshot?.message || 'Checking…'}{percent!=null ? ` ${percent}%` : ''}</p>{/if}
  {#if busy && measured && !preparing}<progress value={$setup.snapshot?.bytes ?? 0} max={$setup.snapshot?.totalBytes ?? 0} aria-label="Progress"></progress>{/if}
  {#if busy && $setup.snapshot?.cancelAvailable && !preparing && !requesting && !planning}<button disabled={$setup.pending || $setup.snapshot?.cancelRequested} onclick={()=>{void setup.cancel();}}>{stopLabel}</button>{/if}
  {#if resultNote}<p role="status">{resultNote}</p>{/if}
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
  .admin{margin:8px 0 0;font-size:12px}.admin .text-action{padding:0 0 0 4px}
  .hint{margin:0 auto 0 0;font-size:11px;color:var(--text-dim)}
  .source-note{color:var(--text-dim)}.source-note::before{content:'·';margin:0 6px}
  .help{margin:0 0 8px}.help p{margin:0 0 6px;font-size:11px}.text-action.inline{padding:0}
  .visually-hidden{position:absolute;width:1px;height:1px;margin:-1px;padding:0;border:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
  .continue{display:grid;grid-template-columns:1fr auto;grid-template-areas:"toggle action" "table table";align-items:center;gap:0 16px;border-top:1px solid var(--border);margin-top:16px;padding-top:14px}.submit{grid-area:action;justify-self:end;display:flex;align-items:center;gap:12px}.waiting{font-size:11px;color:var(--text-dim)}
  .preparing{border:1px solid var(--accent);border-radius:6px;background:var(--surface);padding:22px;margin:24px 0 16px}.preparing h3{margin-bottom:6px}.preparing p{margin:0 0 8px;font-size:12px}
  .progress-steps{list-style:none;margin:10px 0 12px;padding:0;display:grid;gap:8px}.progress-steps li{display:grid;gap:4px;font-size:12px;color:var(--text-dim);padding-left:18px;position:relative}
  .progress-steps li::before{content:'';position:absolute;left:0;top:5px;width:8px;height:8px;border-radius:50%;box-sizing:border-box;border:1.5px solid var(--border-strong)}
  .progress-steps .done{color:var(--text-muted)}.progress-steps .done::before{content:'✓';top:0;border:0;width:auto;height:auto;font-size:11px;color:var(--text-muted)}
  .progress-steps .current{color:var(--text);font-weight:700}.progress-steps .current::before{border-color:var(--accent);background:var(--accent)}
  .progress-steps small{font-size:11px;font-weight:400;color:var(--text-dim)}.progress-steps progress{margin:0}
  .failure{border-left:2px solid var(--error);padding:12px 16px;font-size:12px;margin:16px 0}.failure p{overflow-wrap:anywhere}
  .beside{margin:0}.beside .text-action{padding:0}
  progress{width:100%;height:6px;margin:4px 0 12px;border:0;border-radius:3px;background:var(--surface-raised);appearance:none;-webkit-appearance:none}
  progress::-webkit-progress-bar{border-radius:3px;background:var(--surface-raised)}progress::-webkit-progress-value{border-radius:3px;background:var(--accent)}progress::-moz-progress-bar{background:var(--accent)}
  @keyframes shimmer{from{background-position:100% 0}to{background-position:-100% 0}}
  @media(prefers-reduced-motion:reduce){.map-placeholder{animation:none}}
  @media(max-width:550px){h2{font-size:20px}.disk-row{flex-wrap:wrap}.disk-row .hint{order:3;flex-basis:100%}.preparing{padding:16px}.continue{grid-template-columns:1fr;grid-template-areas:"action" "toggle" "table";gap:10px}.submit{justify-self:stretch;flex-direction:column-reverse;align-items:stretch}}
</style>
