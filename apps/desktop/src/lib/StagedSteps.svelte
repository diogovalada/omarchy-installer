<script lang="ts">
  import type { StagedOperation } from './setup';
  // A prepared temporary installer as steps; only the current one is spelled
  // out, with one primary button. Used in the flow and on the home screen.
  export let op:StagedOperation;
  // From the disk check; unknown on the home screen.
  export let secureBoot:boolean|null|undefined=undefined;
  export let testing=false;
  export let canPrepareAgain=false;
  export let busy=false;
  // Set while a running command prepares or removes this installer.
  export let underway:'preparing'|'removing'|null=null;
  export let act:(action:'arm'|'firmware'|'cleanup'|'again')=>void=()=>{};
  type Action='arm'|'firmware'|'cleanup'|'again';
  // Shown before restarting, since the app can't be seen from the installer.
  const INSTALLER='In the installer, choose the disk marked “installer disk, free space only”, then “Free space install”.';
  const TEXT=[
    'Restart into firmware settings, turn off Secure Boot, then return to Windows.',
    'Make your next restart open the installer.',
    'Restart Windows to start the installer.',
    'Does your PC now start Omarchy by itself? Then remove the temporary installer. Resume BitLocker when Windows reminds you.'
  ];
  $: ready=['staged','arming','boot-scheduled'].includes(op.status);
  // Without a command on it, an unfinished status was interrupted; an
  // interrupted removal is left `cleaning`.
  $: heading=underway==='removing' ? 'Removing the temporary installer' : underway==='preparing' ? 'Preparing the installer' : ready ? 'Temporary installer' : op.status==='cleaning' ? "Removing the temporary installer didn't finish" : "Preparing the installer didn't finish";
  $: scheduled=op.status==='boot-scheduled';
  // 0 Turn off Secure Boot, 1 Start installer, 2 Install Omarchy, 3 Clean up;
  // -1 when preparation needs attention.
  $: current=!ready ? -1 : scheduled ? (op.restartedSinceScheduled ? 3 : 2) : secureBoot ? 0 : 1;
  $: steps=[secureBoot==null ? "Turn off Secure Boot if it's on" : 'Turn off Secure Boot','Set next restart','Install Omarchy','Clean up'];
  // Startup selection and firmware restarts exist only in testing builds.
  $: available={arm:testing && ready,firmware:testing && ready && secureBoot!==false,cleanup:true,again:canPrepareAgain} as Record<Action,boolean>;
  $: primary=underway ? null : ([current===0 ? 'firmware' : current===1 ? 'arm' : current===2 ? null : 'cleanup'] as (Action|null)[]).find(action=>action && available[action]) ?? null;
  // Starting the installer comes last while it can't help yet: before
  // Secure Boot is off, or once it is already set for the next restart.
  $: secondary=((current===0 || scheduled ? ['firmware','cleanup','again','arm'] : ['arm','firmware','cleanup','again']) as Action[]).filter(action=>!underway && available[action] && action!==primary);
  $: labels={arm:scheduled ? 'Start installer again on next restart' : 'Start installer on next restart',firmware:'Restart to firmware settings',cleanup:'Remove temporary installer',again:'Prepare again'} as Record<Action,string>;
</script>

<article aria-label={`Temporary installer on Disk ${op.diskNumber}`}>
  <h3>{heading} · Disk {op.diskNumber}</h3>
  {#if underway}
    <p>Keep this window open.</p>
  {:else if ready}
    <ol class="steps" aria-label="Steps">
      {#each steps as step,index}
        <li class:done={index<current && !(index===0 && secureBoot==null)} class:maybe={index===0 && secureBoot==null && current>0} class:current={index===current} aria-current={index===current ? 'step' : undefined}>{step}</li>
      {/each}
    </ol>
    <p>{TEXT[current]}</p>
    {#if current===0}<details class="how"><summary>How?</summary><p>Look for a Security or Boot tab, set Secure Boot to Disabled, then save and exit.</p></details>{/if}
    {#if current===1 || current===2}<p>{INSTALLER}</p>{/if}
  {:else}
    <p>Remove the temporary installer. Windows and any installed Omarchy stay.</p>
  {/if}
  {#if primary}<button class="primary" disabled={busy} onclick={()=>act(primary)}>{labels[primary]}</button>{/if}
  {#if ready && !underway}
    <details>
      <summary>Starting Windows later</summary>
      <p>Omarchy starts by default after installing. To open Windows, use your computer's boot menu key (often F12, F11 or Esc), or run <code>limine-scan</code> in Omarchy once to add Windows to its menu.</p>
      <p>If you replace Windows, the temporary installer stays on the disk.</p>
    </details>
  {/if}
  {#if secondary.length}
    <div class="secondary">{#each secondary as action}<button class="text-action" disabled={busy} onclick={()=>act(action)}>{labels[action]}</button>{/each}</div>
  {/if}
</article>

<style>
  article{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:22px;margin:24px 0 16px}
  h3{font-size:13px;margin:0 0 14px}
  .steps{display:flex;flex-wrap:wrap;gap:6px 20px;margin:0 0 14px;padding:0;list-style:none;font-size:11px;color:var(--text-dim)}
  .steps li{display:flex;align-items:center;gap:6px}
  .steps li::before{content:'';width:8px;height:8px;border-radius:50%;box-sizing:border-box;border:1.5px solid var(--border-strong)}
  .steps .done{color:var(--text-muted)}.steps .done::before{content:'✓';display:grid;place-items:center;border:0;color:var(--text-muted);font-size:10px}
  .steps .maybe::before{border-style:dashed}
  .steps .current{color:var(--text);font-weight:700}.steps .current::before{border-color:var(--accent);background:var(--accent)}
  p{margin:0 0 8px;font-size:12px;line-height:1.8;color:var(--text-muted)}
  button{display:inline-flex;align-items:center;justify-content:center;min-height:42px;margin:6px 0 4px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:disabled{opacity:.5;cursor:not-allowed}
  .primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  details{margin:10px 0 0;font-size:11px;color:var(--text-dim);line-height:1.8}summary{cursor:pointer;padding:4px 0}details p{font-size:11px}
  .how{margin:-4px 0 4px}code{padding:0 4px;border-radius:3px;background:var(--surface-raised);font-size:11px;color:var(--text)}
  .secondary{display:flex;flex-wrap:wrap;gap:4px 18px;margin-top:12px;padding-top:12px;border-top:1px solid var(--border)}
  .text-action{min-height:0;margin:0;padding:0;border:0;background:transparent;color:var(--accent);font-size:11px;text-decoration:underline;text-underline-offset:3px}.text-action:hover:not(:disabled){color:var(--accent-hover)}
  @media(max-width:550px){article{padding:16px}}
</style>
