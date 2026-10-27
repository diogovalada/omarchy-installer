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
  export let act:(action:Action)=>void=()=>{};
  type Action='restart'|'arm'|'firmware'|'cleanup'|'again';
  // Shown before restarting, since the app can't be seen from the installer.
  // A replaced partition (Replace Windows) is deleted there to make room.
  $: installer=`In the installer, choose the disk marked “installer disk, free space only”, then “Free space install”${op.replaces ? `, and delete ${op.replaces} when it asks for more space` : ''}.`;
  // The Secure Boot step (0) is spelled out below.
  const TEXT=[
    '',
    'Restart into the installer when you’re ready.',
    'The installer opens at your next restart.',
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
  $: steps=[secureBoot==null ? "Turn off Secure Boot if it's on" : 'Turn off Secure Boot','Start the installer','Install Omarchy','Clean up'];
  // Startup selection and firmware restarts exist only in testing builds.
  $: available={restart:testing && ready,arm:testing && ready,firmware:testing && ready && secureBoot!==false,cleanup:true,again:canPrepareAgain} as Record<Action,boolean>;
  // Restarting into the installer also retries a selection whose restart
  // hasn't happened (step 2).
  $: primary=underway ? null : ([current===0 ? 'firmware' : current===1 || current===2 ? 'restart' : 'cleanup'] as Action[]).find(action=>available[action]) ?? null;
  // Starting the installer comes last while it can't help yet: before
  // Secure Boot is off, or once it is already set for the next restart.
  $: secondary=((current===0 ? ['cleanup','again','arm'] : current===1 ? ['arm','firmware','cleanup','again'] : current===2 ? ['firmware','cleanup','again'] : ['firmware','cleanup','again','arm']) as Action[]).filter(action=>!underway && available[action] && action!==primary);
  $: labels={restart:'Restart into the installer',arm:scheduled ? 'Start installer again on next restart' : 'Set for next restart only',firmware:'Restart to firmware settings',cleanup:'Remove temporary installer',again:'Prepare again'} as Record<Action,string>;
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
    {#if current===0}
      <p>Restart into your PC’s firmware settings, then:</p>
      <ol class="how-to">
        <li>Find <strong>Secure Boot</strong>, usually under Security, Boot or Authentication.</li>
        <li>Set it to <strong>Disabled</strong> and leave TPM on.</li>
        <li>Save and exit, often with F10.</li>
      </ol>
      <p>If the firmware warns that BitLocker may ask for a recovery key, you can continue: this app paused BitLocker.</p>
      <p>Windows then starts again and this app reopens, so you can start the installer. Windows may ask you to set up your PIN again, so make sure you can sign in another way, such as your password or your Microsoft account’s recovery email or phone.</p>
    {:else}<p>{TEXT[current]}</p>{/if}
    {#if current===1 || current===2}<p>{installer}</p>{/if}
  {:else}
    <p>Remove the temporary installer. Windows and any installed Omarchy stay.</p>
  {/if}
  {#if primary}<button class="primary" disabled={busy} onclick={()=>act(primary)}>{labels[primary]}</button>{/if}
  <!-- Below the button, so opening it moves nothing the user might click. -->
  {#if ready && !underway && current===0}<details class="how"><summary>Can’t change it?</summary><p>Some PCs only unlock Secure Boot after you set a firmware password, or call the setting “OS Type” (choose Other OS).</p></details>{/if}
  {#if ready && !underway}
    <details>
      <summary>Starting Windows later</summary>
      <p>Omarchy starts by default after installing. To open Windows, press your computer's boot menu key as it starts (often F12, F11 or Esc) and choose Windows Boot Manager.</p>
      <!-- An entry that hands over to the firmware's Windows Boot Manager, not
           limine-scan's chainload: with Secure Boot off, BitLocker measures a
           chainloading Limine, so each Limine update would ask for the key. -->
      <p>To add Windows to Omarchy's menu instead, add these lines at the end of <code>/boot/limine.conf</code> in Omarchy:</p>
      <pre><code>/Windows
    protocol: efi_boot_entry
    entry: Windows Boot Manager</code></pre>
      <p>Windows then starts after one quick restart, and BitLocker won't ask for its recovery key when Omarchy updates its boot menu. If <code>sudo efibootmgr</code> doesn't list Windows Boot Manager, run <code>limine-scan</code> instead. Omarchy may hide its menu: press any key as soon as your PC starts to show it.</p>
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
  .how{margin:-4px 0 4px}.how-to{margin:0 0 8px;padding-left:20px;font-size:12px;line-height:1.8;color:var(--text-muted)}.how-to strong{color:var(--text)}code{padding:0 4px;border-radius:3px;background:var(--surface-raised);font-size:11px;color:var(--text)}pre{margin:0 0 8px;padding:8px 10px;border-radius:4px;background:var(--surface-raised);overflow-x:auto}pre code{padding:0;background:none;line-height:1.6}
  .secondary{display:flex;flex-wrap:wrap;gap:4px 18px;margin-top:12px;padding-top:12px;border-top:1px solid var(--border)}
  .text-action{min-height:0;margin:0;padding:0;border:0;background:transparent;color:var(--accent);font-size:11px;text-decoration:underline;text-underline-offset:3px}.text-action:hover:not(:disabled){color:var(--accent-hover)}
  @media(max-width:550px){article{padding:16px}}
</style>
