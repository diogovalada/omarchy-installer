<script lang="ts">
  import { ArrowLeft, Check, RefreshCw } from 'lucide-svelte';
  import { downloads, formatBytes } from './downloads';
  import { setup, setupActive, usbIssueSummary } from './setup';
  import StorageControls from './StorageControls.svelte';
  import BootMenuSettings from './BootMenuSettings.svelte';
  export let kind: 'usb' | 'direct';
  export let close: () => void;
  let selected = '';
  let defaultOs:'omarchy'|'windows'='omarchy';
  let timeoutSeconds=5;
  let firmwareReady=false;
  $: snapshot = $setup.snapshot?.kind === kind ? $setup.snapshot : null;
  $: active = setupActive(snapshot?.status) || $setup.pending;
  $: complete = snapshot?.status === 'complete';
  $: readyImage = $downloads.snapshot?.status === 'complete' && !!$downloads.snapshot.image_path;
  $: selectedChoice = snapshot?.choices.find(choice => choice.id === selected && choice.eligible);
  $: percent = snapshot?.totalBytes ? Math.min(100, snapshot.bytes / snapshot.totalBytes * 100) : 0;
  $: error = $setup.error ?? snapshot?.error;
  $: usbReview = snapshot?.usbReview;
  const focusReview = (node:HTMLElement) => { node.focus(); };
  function confirmUsb() {
    if (!usbReview || active) return;
    void setup.start(usbReview.choiceId,undefined,undefined,undefined,usbReview.mode,usbReview.token);
  }
</script>

<section class="flow" aria-label={kind === 'usb' ? 'Create bootable USB' : 'Install without USB'}>
  <div class="heading"><h2>{kind === 'usb' ? 'Create bootable USB' : 'Install without USB'}</h2><button class="back" disabled={active} onclick={()=>{if(usbReview)void setup.dismissUsbReview();close();}}><ArrowLeft size={15}/>Back</button></div>
  {#if !downloads.native}
    <p>Open the desktop app to inspect disks and continue.</p>
  {:else if !readyImage}
    <p>Download and verify the Omarchy image above to continue.</p>
  {:else if complete}
    <div class="done" role="status"><Check size={21}/>{kind === 'usb' ? 'Bootable USB created and verified.' : 'Omarchy has been installed.'}</div>
    {#if kind === 'usb'}
      <p>{snapshot?.receipt?.receipt?.message ?? snapshot?.receipt?.receipt?.eject?.message ?? 'The image was written and read back successfully.'}</p>
      {#if snapshot?.receipt?.receipt?.backupPath}<p>Boot recovery archive: <span class="path">{snapshot.receipt.receipt.backupPath}</span></p>{/if}
      <button onclick={() => {selected='';void setup.inspect('usb');}}>Check USB drives</button>
    {:else}
      <p>When you are ready, restart. The boot menu offers Omarchy and Windows and starts your chosen default after the countdown. Choosing Windows triggers a brief extra restart. On Omarchy’s first boot, it completes hardware setup and replaces its temporary encryption key with your account password.</p>
      {#if snapshot?.receipt?.receipt?.bitLockerRestoration?.required && snapshot.receipt.receipt.bitLockerRestoration.verified}<p>Windows BitLocker protection has been restored and verified.</p>{/if}
    {/if}
    {#if snapshot?.receipt?.receiptPath}<details><summary>Operation receipt</summary><p class="path">{snapshot.receipt.receiptPath}</p></details>{/if}
  {:else}
    <p>{kind === 'usb' ? 'Choose a USB drive.' : 'Choose where to make space for Omarchy and how much to allocate. Windows is retained. A boot menu will let you choose either OS at startup.'}</p>
    {#if kind === 'direct'}
      <p class="requirements">Omarchy uses LUKS2 encryption. Your password replaces the temporary setup key on first boot. If Windows uses BitLocker, its data remains encrypted while protection is temporarily suspended where needed and restored afterward.</p>
      <p class="requirements">Requires at least about 40 GiB for Omarchy and Secure Boot off. Local preparation needs the construction runtime, 85 GiB of working space and 10 GiB of free memory.</p>
    {/if}
    {#if active}
      <p role="status" aria-live="polite">{snapshot?.cancelRequested ? 'Stopping safely…' : snapshot?.message ?? 'Checking this computer…'}</p>
      {#if snapshot?.totalBytes}<progress max="100" value={percent} aria-label="Setup progress"></progress><p class="muted">{formatBytes(snapshot.bytes)} / {formatBytes(snapshot.totalBytes)}</p>{:else}<progress max="100" aria-label="Setup progress"></progress>{/if}
      {#if snapshot?.cancelAvailable}<div class="actions"><button disabled={snapshot.cancelRequested || $setup.pending} onclick={() => { void setup.cancel(); }}>Cancel</button></div>{:else if snapshot?.status === 'running'}<p class="muted">Keep the computer powered on. Cancellation is unavailable during this stage.</p>{/if}
    {:else if kind === 'usb' && usbReview}
      <section class="usb-confirmation" aria-labelledby="usb-confirm-title">
        <h3 id="usb-confirm-title" tabindex="-1" use:focusReview>{usbReview.mode === 'erase' ? 'Erase this USB?' : 'Add the installer to this USB?'}</h3>
        <p class="review-target">{usbReview.label} · {formatBytes(usbReview.sizeBytes)}<small>{usbReview.detail}</small></p>
        <p>Installer: {usbReview.fileName}</p>
        {#if usbReview.mode === 'erase'}
          <p class="error">All files and partitions on this USB will be deleted.</p>
        {:else}
          <p>Your files and partitions will be kept.</p>
          {#if usbReview.replacesBootloader}<p>The current bootloader will be backed up and replaced. Automatic restoration is unavailable.</p>{/if}
          <p>Boot using x64 UEFI with Secure Boot off.</p>
        {/if}
        <p class="muted">Windows will ask for administrator access next.</p>
        <div class="actions"><button onclick={()=>{void setup.dismissUsbReview();}}>Cancel</button><button class:danger={usbReview.mode === 'erase'} class:primary={usbReview.mode === 'preserve'} onclick={confirmUsb}>{usbReview.mode === 'erase' ? 'Erase USB and create installer' : 'Keep files and add installer'}</button></div>
      </section>
    {:else}
      {#if snapshot?.status === 'prepared'}<p role="status">{snapshot.message}</p>{/if}
      {#if kind === 'direct' && snapshot?.preparation?.secureBoot === 'enabled'}
        <div class="preparation">
          <h3>Prepare Secure Boot</h3>
          <p>This Omarchy release needs Secure Boot off. This step restarts into your computer’s firmware settings. Turn off Secure Boot, leave TPM enabled, save, and return to Windows before installing.</p>
          <p>Active Windows BitLocker protection is temporarily suspended for the change and restored when Windows returns. If the restart is cancelled, recovery restores protection after five minutes. An existing suspension is preserved.</p>
          <label><input type="checkbox" bind:checked={firmwareReady}/>I have saved my work and can access my Windows recovery key if needed.</label>
          <button disabled={!firmwareReady} onclick={() => {firmwareReady=false; void setup.prepare('firmware');}}>Prepare and restart to firmware</button>
        </div>
      {/if}
      {#if kind === 'direct' && snapshot?.preparation && !snapshot.preparation.runtimeReady}
        <div class="preparation">
          <h3>Prepare the construction runtime</h3>
          <p>Install and start Docker Desktop with its Linux engine, then import the construction runtime included with this app. Docker may require Windows setup and a restart. The app checks again before installing Omarchy.</p>
          {#if snapshot.preparation.runtimePackaged}<button onclick={() => {void setup.prepare('runtime');}}>Import packaged runtime</button>{:else}<p>This development build does not include the runtime archive. Use a package with the construction runtime included.</p>{/if}
        </div>
      {/if}
      {#if snapshot?.requirements.length}<ul class="requirements">{#each snapshot.requirements as requirement}<li>{requirement}</li>{/each}</ul>{/if}
      {#if kind === 'direct'}
        <StorageControls choices={snapshot?.choices ?? []}
          helpTitle="ext4 and Btrfs partitions can’t be resized here"
          helpText="This installer can shrink supported NTFS partitions on Windows. To make room on an ext4 or Btrfs partition, resize it in Linux and leave the freed space unallocated. Then return here and select Refresh disks."
          prepare={(choice,bytes,confirmation)=>{void setup.start(choice.id,bytes,confirmation,{defaultOs,timeoutSeconds});}}>
          <BootMenuSettings bind:defaultOs bind:timeoutSeconds/>
        </StorageControls>
      {:else}
        <div class="choices">
          {#each snapshot?.choices ?? [] as choice}
            <label class:unavailable={!choice.eligible} class:selected={selected===choice.id}>
              <input type="radio" name="setup-target" value={choice.id} bind:group={selected} disabled={!choice.eligible} onchange={()=>{void setup.inspectUsb(choice.id);}}/>
              <span><strong>{choice.label || 'USB drive'}</strong><small>{choice.detail} · {formatBytes(choice.sizeBytes)}</small>{#each choice.reasons as reason}<small class="reason">{reason}</small>{/each}</span>
            </label>
          {/each}
        </div>
        {#if selectedChoice}
          <div class="usb-options">
            <h3>How should this USB be prepared?</h3>
            {#if selectedChoice.usb?.freeBytes != null}<p class="muted">{formatBytes(selectedChoice.usb.freeBytes)} free · {formatBytes(selectedChoice.usb.requiredBytes)} needed to keep files</p>{/if}
            <div class="usb-option">
              <button class="primary" disabled={!readyImage || !selectedChoice.usb?.eligible} onclick={()=>{void setup.reviewUsb(selected,'preserve');}}>Keep files and add installer</button>
              {#if selectedChoice.usb && !selectedChoice.usb.eligible}
                <p role="status"><span class="error">Can’t keep files on this USB.</span> {usbIssueSummary(selectedChoice.usb)}</p>
                {#if selectedChoice.usb.reasons.length}<details><summary>Details</summary>{#each selectedChoice.usb.reasons as reason}<p>{reason}</p>{/each}</details>{/if}
              {/if}
              {#if !selectedChoice.usb}<p class="muted">Check this USB’s compatibility before keeping files.</p><button onclick={()=>{void setup.inspectUsb(selected);}}>Check compatibility</button>{/if}
              {#if selectedChoice.usb?.needsAdministrator}<button onclick={()=>{void setup.inspectUsb(selected,true);}}>Check compatibility with administrator access</button>{/if}
            </div>
            <div class="usb-option">
              <button class="erase" disabled={!readyImage} onclick={()=>{void setup.reviewUsb(selected,'erase');}}>Erase USB and create installer</button>
              <p>Deletes all files on this USB.</p>
            </div>
          </div>
        {/if}
      {/if}
      {#if snapshot?.status==='ready' && !snapshot.choices.length}<p class="muted">{kind==='usb' ? 'No eligible USB drives found. Connect a USB drive and refresh.' : 'No eligible disks found.'}</p>{/if}
      <div class="actions">
        <button onclick={() => { selected=''; void setup.inspect(kind); }}><RefreshCw size={14}/>{snapshot ? 'Refresh disks' : 'Check disks'}</button>
      </div>
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if snapshot?.recovery && !active}
    <details><summary>{error ? 'Recovery and operation records' : 'Temporary files and operation records'}</summary>
      {#if snapshot.recovery.mutationStarted && error}<p>Storage changes started. Keep the records and inspect the planned partition sizes before retrying. If needed, select the existing Windows Boot Manager entry in firmware to return to Windows.</p>{/if}
      <p>{snapshot.recovery.cleanup.complete ? 'Known temporary construction files were cleaned up. Operation records are retained.' : 'Some temporary files remain. Keep the operation records and confirm construction has stopped before removing anything.'}</p>
      <p class="path">{snapshot.recovery.filesPath}</p>
    </details>
  {/if}
</section>


<style>
  .flow{margin-top:24px;padding:24px;border:1px solid var(--border);border-radius:4px;background:var(--surface)}
  .heading{display:flex;align-items:center;justify-content:space-between;gap:16px}h2{font-size:16px;font-weight:400;margin:0}
  p,li{font-size:11px;line-height:1.8;color:var(--text-muted)}p{margin:18px 0}.requirements,.muted{color:var(--text-dim);font-size:10px}.requirements{padding-left:0}.requirements li{margin-left:16px;color:var(--text-dim)}
  button{display:inline-flex;align-items:center;justify-content:center;gap:8px;min-height:38px;padding:9px 13px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:disabled{opacity:.5;cursor:not-allowed}button:hover:not(:disabled){border-color:var(--accent)}button.back{border:0;background:none;color:var(--text-dim);padding:0;min-height:30px}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep)}
  .actions{display:flex;gap:12px;flex-wrap:wrap;margin-top:20px}.choices{display:grid;gap:10px}.choices label{display:flex;align-items:flex-start;gap:12px;padding:14px;border:1px solid var(--border);border-radius:3px;cursor:pointer}.choices label.selected{border-color:var(--accent)}.choices label.unavailable{cursor:not-allowed}.choices input{margin-top:4px;accent-color:var(--accent)}.choices span{min-width:0}.choices strong{font-size:12px;font-weight:400;overflow-wrap:anywhere}.choices small{display:block;color:var(--text-muted);font-size:10px;line-height:1.8;margin-top:5px}.choices small.reason{color:var(--text-dim)}.unavailable strong{color:var(--text-muted)}
  progress{display:block;width:100%;height:6px;accent-color:var(--accent);border:0;background:var(--surface-deep)}progress::-webkit-progress-bar{background:var(--surface-deep)}progress::-webkit-progress-value{background:var(--accent)}.done{display:flex;align-items:center;gap:10px;color:var(--success);font-size:13px;margin-top:24px}.error{color:var(--error);overflow-wrap:anywhere}.path{overflow-wrap:anywhere}summary{font-size:10px;color:var(--text-dim);cursor:pointer}
  @media(max-width:550px){.flow{padding:20px}.heading{align-items:flex-start}h2{font-size:14px}}
  .preparation{border:1px solid var(--border);padding:16px;margin-top:18px}.preparation h3{font-size:13px;font-weight:400;margin:0}.preparation label{display:flex;align-items:flex-start;gap:8px;font-size:11px;line-height:1.7;color:var(--text-muted);margin:12px 0}.preparation input{accent-color:var(--accent)}
  .usb-confirmation{border:1px solid var(--border-strong);padding:20px;margin-top:18px}.usb-confirmation h3{font-size:16px;font-weight:400;margin:0}.usb-confirmation .review-target{color:var(--text);font-size:13px}.review-target small{display:block;font-size:10px;color:var(--text-dim);margin-top:6px}.danger{background:var(--error);border-color:var(--error);color:var(--surface-deep)}
  .usb-options{margin-top:22px}.usb-options h3{font-size:12px;font-weight:400}.usb-option{border-top:1px solid var(--border);padding:18px 0 4px}.usb-option p{margin:10px 0}.erase{color:var(--error)}
</style>
