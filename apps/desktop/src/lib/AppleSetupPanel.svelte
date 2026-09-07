<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowLeft } from 'lucide-svelte';
  import { appleSetup } from './appleSetup';
  import { formatBytes } from './downloads';
  export let close:()=>void;
  import StorageControls from './StorageControls.svelte';
  import type { StorageChoice } from './storageChoices';
  $: snapshot=$appleSetup.snapshot;
  $: native=snapshot?.native;
  $: phase=native?.phase ?? 'idle';
  $: plan=snapshot?.plan ?? native?.plan;
  $: busy=!!snapshot?.busy || !!native?.busy || $appleSetup.pending;
  $: error=$appleSetup.error ?? snapshot?.error?.message;
  $: choices=native?.storageChoices ?? [];
  function prepare(choice:StorageChoice,bytes:number|undefined,confirmation?:string) {
    const allocation=bytes===undefined ? {} : {allocation_bytes:String(bytes)};
    void appleSetup.action(phase==='welcome' ? {kind:'prepare_plan',...allocation} : {kind:'choose_storage',choice_id:choice.id,...allocation,...(confirmation ? {confirmation} : {})});
  }
  onMount(()=>{void appleSetup.refresh();});
</script>

<section class="flow" aria-label="Install without USB on Mac">
  <div class="heading"><h2>Install without USB</h2><button class="back" disabled={busy || !!native?.hasExecutionStarted} onclick={close}><ArrowLeft size={15}/>Back</button></div>
  <p>Choose storage for Omarchy using the native Apple installer. It downloads a separate Apple Silicon image and guides you through the required Recovery steps.</p>
  <details><summary>How startup works on this Mac</summary><p>Apple Silicon uses Apple’s startup picker. Shut down and hold the power button until startup options appear, then choose Omarchy or macOS. Normal startup uses your configured startup disk. The Windows PC countdown menu does not apply here.</p><p>Keep macOS installed for native installation and maintenance. Replacing an existing Omarchy installation does not delete macOS. Follow the Finish Installation steps in Recovery before the first normal Omarchy boot.</p></details>
  {#if !snapshot || snapshot.status==='idle' || snapshot.status==='unavailable' || (!snapshot.available && !native)}
    <p class="muted">Requires a compatible Apple Silicon Mac and the packaged, signed native installer.</p>
    <button disabled={busy} onclick={()=>{void appleSetup.action({kind:'connect'});}}>Check installer</button>
  {:else}
    {#each snapshot.probe?.blockers ?? [] as blocker}<p class="error">{blocker.message}</p>{/each}
    {#if native?.host}<p>{native.host.model} · {native.host.chip}</p>{/if}
    {#if native?.hostSummary}<p>{native.hostSummary}</p>{/if}
    {#if native?.blockingReason}<p class="error">{native.blockingReason}</p>{/if}
    {#if native?.admissionBlock}<p class="error">{native.admissionBlock.message}</p>{/if}
    {#if native?.helper && !native.helper.enabled}<p class="muted">{native.helper.summary}</p>{/if}
    {#if busy}
      <p role="status" aria-live="polite">{native?.progress?.phaseTitle ?? native?.progress?.stage?.replace(/_/g,' ') ?? snapshot?.pending_intent?.replace(/_/g,' ') ?? 'Preparing…'}</p>
      <progress max="100" aria-label="Installation progress"></progress>
      {#each native?.progress?.rows ?? [] as row}<p class="muted">{row.fileName} · {formatBytes(Number(row.bytesCompleted))} / {formatBytes(Number(row.totalBytes))}</p>{/each}
      <p class="muted">Keep the computer powered on. This stage cannot be cancelled.</p>
    {/if}
    {#if plan && ['plan_prepared','plan_review','awaiting_install'].includes(phase)}
      <h3>{plan.headline}</h3><p>{plan.subheadline}</p>
      {#if plan.deletion}<p class="error">This plan deletes {plan.deletion.identifier}: {plan.deletion.sourceIdentifier} ({formatBytes(Number(plan.deletion.sizeBytes))}), including its operating system and files. This cannot be undone.</p>{/if}
      <dl>{#each plan.facts as fact}<div><dt>{fact.label}</dt><dd>{fact.value}</dd></div>{/each}<div><dt>Omarchy</dt><dd>{formatBytes(Number(plan.omarchyBytes))}</dd></div><div><dt>macOS</dt><dd>{formatBytes(Number(plan.macOSBytes))}</dd></div></dl>
      <details><summary>Verified installation files</summary>{#each plan.artifacts as artifact}<p class="muted">{artifact.fileName} · {formatBytes(Number(artifact.expectedBytes))}</p>{/each}<p class="muted digest">Plan binding: {plan.bindingDigest}</p></details>
    {/if}
    {#if native?.handoff}
      <h3>{native.handoff.headline}</h3><p>{native.handoff.subheadline}</p><p>{native.handoff.explainer}</p>
      <ol>{#each native.handoff.steps as step}<li><strong>{step.title}</strong><p>{step.detail}</p></li>{/each}</ol><p>{native.handoff.hint}</p>
    {:else if phase==='done'}<h3>{native?.completion?.headline}</h3><p>{native?.completion?.subheadline}</p>{/if}
    {#if native?.failure}<div role="alert"><h3>{native.failure.headline}</h3><p class="error">{native.failure.detail}</p>{#if native.failure.remedy}<p>{native.failure.remedy}</p>{/if}{#if native.failure.technicalDetail}<details><summary>Details</summary><p class="digest">{native.failure.technicalDetail}</p></details>{/if}</div>{/if}
    {#if native?.credentials?.rejected}<p class="error">The credentials were rejected. Enter the machine owner’s credentials again to continue.</p>{/if}
    {#if !busy && !snapshot.outcome_unknown}
      {#if ['welcome','existing_install_choice'].includes(phase)}
        <StorageControls {choices} {prepare}
          helpTitle="NTFS, ext4 and Btrfs partitions can’t be resized here"
          helpText="The native installer selects suitable free space or resizes macOS storage. To resize NTFS, use Windows. For ext4 or Btrfs, use Linux. Existing Omarchy installations can be replaced when the native installer identifies them."/>
      {/if}
      <div class="actions">
        {#if phase==='idle'}<button onclick={()=>{void appleSetup.action({kind:'inspect'});}}>Check this Mac</button>
        {:else if phase==='plan_prepared'}<button class="primary" onclick={()=>{void appleSetup.action({kind:'review_plan'});}}>Review installation</button>
        {:else if phase==='plan_review'}<button class="primary" onclick={()=>{void appleSetup.action({kind:'approve_plan'});}}>Confirm this plan</button>
        {:else if phase==='awaiting_install'}<button class="primary" disabled={!native?.canExecute} onclick={()=>{void appleSetup.action({kind:'execute'});}}>Install Omarchy</button>{/if}
        {#if native?.canRetryRecovery}<button onclick={()=>{void appleSetup.action({kind:'retry_recovery'});}}>Continue Recovery authorization</button>{/if}
        {#if ['welcome','existing_install_choice'].includes(phase)}<button onclick={()=>{void appleSetup.action({kind:'inspect'});}}>Refresh disks</button>{/if}
        {#if !native?.hasExecutionStarted}<button onclick={()=>{void appleSetup.action({kind:'refresh'});}}>Refresh installer status</button>{/if}
        {#if native?.cancelAvailable && phase!=='idle' && phase!=='welcome'}<button onclick={()=>{void appleSetup.action({kind:'cancel'});}}>Discard preparation</button>{/if}
      </div>
    {/if}
  {/if}
  {#if snapshot?.outcome_unknown}<p class="error" role="alert">The installer connection was interrupted after submission. The disk’s state is unknown. Preserve the installation journal and follow the native Recovery guidance before retrying.</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .flow{margin-top:24px;padding:24px;border:1px solid var(--border);border-radius:4px;background:var(--surface)}.heading{display:flex;align-items:center;justify-content:space-between;gap:16px}h2{font-size:16px;font-weight:400;margin:0}h3{font-size:14px;font-weight:400;margin:24px 0 12px}p,li{font-size:11px;line-height:1.8;color:var(--text-muted)}p{margin:18px 0}.muted{color:var(--text-dim);font-size:10px}.error{color:var(--error);overflow-wrap:anywhere}
  button{display:inline-flex;align-items:center;justify-content:center;gap:8px;min-height:38px;padding:9px 13px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:disabled{opacity:.5;cursor:not-allowed}button:hover:not(:disabled){border-color:var(--accent)}button.back{border:0;background:none;color:var(--text-dim);padding:0;min-height:30px}.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep)}.actions{display:flex;align-items:flex-end;gap:12px;flex-wrap:wrap;margin-top:20px}
  dl{font-size:11px;line-height:1.8}dl>div{display:flex;justify-content:space-between;gap:24px;padding:9px 0;border-bottom:1px solid var(--border)}dt{color:var(--text-dim)}dd{margin:0;text-align:right;overflow-wrap:anywhere;max-width:65%}summary{font-size:10px;color:var(--text-dim);cursor:pointer}.digest{overflow-wrap:anywhere}progress{display:block;width:100%;height:6px;accent-color:var(--accent);border:0;background:var(--surface-deep)}progress::-webkit-progress-bar{background:var(--surface-deep)}progress::-webkit-progress-value{background:var(--accent)}ol{padding-left:20px}li p{margin:4px 0 16px}li strong{font-weight:400;color:var(--text)}
  @media(max-width:550px){.flow{padding:20px}h2{font-size:14px}dl>div{gap:12px}}
</style>
