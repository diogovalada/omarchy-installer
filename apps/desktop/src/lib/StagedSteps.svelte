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
  export let act:(action:'arm'|'firmware'|'cleanup'|'again')=>void=()=>{};
  type Action='arm'|'firmware'|'cleanup'|'again';
  // Shown before restarting, since the app can't be seen from the installer.
  const INSTALLER='In the installer, choose the disk marked “installer disk, free space only”, then Free space install.';
  const TEXT=[
    'Restart into firmware settings, turn off Secure Boot, then return to Windows.',
    'Choose the installer for your next restart.',
    'Restart Windows to start the installer.',
    'If Omarchy starts on its own, resume BitLocker when reminded, then remove the temporary installer.'
  ];
  $: ready=['staged','arming','boot-scheduled'].includes(op.status);
  $: scheduled=op.status==='boot-scheduled';
  // 0 Turn off Secure Boot, 1 Start installer, 2 Install Omarchy, 3 Clean up;
  // -1 when preparation needs attention.
  $: current=!ready ? -1 : scheduled ? (op.restartedSinceScheduled ? 3 : 2) : secureBoot ? 0 : 1;
  $: steps=[secureBoot==null ? "Turn off Secure Boot if it's on" : 'Turn off Secure Boot','Start installer','Install Omarchy','Clean up'];
  // Startup selection and firmware restarts exist only in testing builds.
  $: available={arm:testing && ready,firmware:testing && ready && secureBoot!==false,cleanup:true,again:canPrepareAgain} as Record<Action,boolean>;
  $: primary=([current===0 ? 'firmware' : current===1 ? 'arm' : current===2 ? null : 'cleanup'] as (Action|null)[]).find(action=>action && available[action]) ?? null;
  $: secondary=(['arm','firmware','cleanup','again'] as Action[]).filter(action=>available[action] && action!==primary);
  $: labels={arm:scheduled ? 'Start installer again on next restart' : 'Start installer on next restart',firmware:'Restart to firmware settings',cleanup:'Remove temporary installer',again:'Prepare again'} as Record<Action,string>;
</script>

<article aria-label={`Temporary installer on Disk ${op.diskNumber}`}>
  <h3>{ready ? 'Temporary installer ready' : 'Installer preparation needs attention'} · Disk {op.diskNumber}</h3>
  {#if ready}
    <ol class="steps" aria-label="Steps">
      {#each steps as step,index}
        <li class:done={index<current && !(index===0 && secureBoot==null)} class:current={index===current} aria-current={index===current ? 'step' : undefined}>{step}</li>
      {/each}
    </ol>
    <p>{TEXT[current]}</p>
    {#if current===1 || current===2}<p>{INSTALLER}</p>{/if}
  {:else}
    <p>Removing it keeps Windows and any installed Omarchy.</p>
  {/if}
  {#if primary}<button class="primary" disabled={busy} onclick={()=>act(primary)}>{labels[primary]}</button>{/if}
  {#if ready}
    <details>
      <summary>Starting Windows later</summary>
      <p>Omarchy starts by default after installing. To open Windows, use your computer’s boot menu key (often F12, F11 or Esc), or run limine-scan in Omarchy once to add Windows to its menu.</p>
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
  .steps .done{color:var(--text-muted)}.steps .done::before{background:var(--border-strong)}
  .steps .current{color:var(--text);font-weight:700}.steps .current::before{border-color:var(--accent);background:var(--accent)}
  p{margin:0 0 8px;font-size:12px;line-height:1.8;color:var(--text-muted)}
  button{display:inline-flex;align-items:center;justify-content:center;min-height:42px;margin:6px 0 4px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:11px;cursor:pointer}button:disabled{opacity:.5;cursor:not-allowed}
  .primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep);font-weight:700}.primary:hover:not(:disabled){background:var(--accent-hover);border-color:var(--accent-hover)}
  details{margin:10px 0 0;font-size:11px;color:var(--text-dim);line-height:1.8}summary{cursor:pointer;padding:4px 0}details p{font-size:11px}
  .secondary{display:flex;flex-wrap:wrap;gap:4px 18px;margin-top:12px;padding-top:12px;border-top:1px solid var(--border)}
  .text-action{min-height:0;margin:0;padding:0;border:0;background:transparent;color:var(--accent);font-size:11px;text-decoration:underline;text-underline-offset:3px}.text-action:hover:not(:disabled){color:var(--accent-hover)}
  @media(max-width:550px){article{padding:16px}}
</style>
