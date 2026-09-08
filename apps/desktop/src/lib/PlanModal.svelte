<script lang="ts">
  import { Check, FlaskConical, ShieldCheck, X } from 'lucide-svelte';
  import type { HostProfile } from './model';
  export let title: string;
  export let host: HostProfile;
  export let close: () => void;
  let complete = false;
  const run = () => { complete = true; };
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.currentTarget === event.target && close()}>
  <section class="modal" role="dialog" aria-modal="true" aria-labelledby="plan-title">
    <button class="close" aria-label="Close plan" onclick={close}><X size={17}/></button>
    <span class="modal-icon">{#if complete}<Check size={22}/>{:else}<FlaskConical size={22}/>{/if}</span>
    <p class="eyebrow">Dry-run plan</p><h2 id="plan-title">{complete ? 'Simulation complete' : title}</h2><p class="intro">{complete ? 'The journey completed without accessing a disk, network, provider, or privileged command.' : `Review the exact actions Omarchy Installer would request on ${host.label}.`}</p>
    {#if !complete}
      <div class="digest"><span>Plan digest</span><code>SIM-{host.id.toUpperCase()}-7A19C2</code></div>
      <ol><li><span>1</span><div><strong>Resolve trusted metadata</strong><small>Use a bundled simulation catalog.</small></div></li><li><span>2</span><div><strong>Validate the complete plan</strong><small>Check host, inputs, limits, and provider compatibility.</small></div></li><li><span>3</span><div><strong>Emit simulated progress</strong><small>No system call or external process is available to this build.</small></div></li></ol>
      <div class="safe"><ShieldCheck size={16}/><span><strong>No elevated access</strong><small>Administrator approval will not be requested.</small></span></div>
      <button class="primary" onclick={run}>Run simulation</button>
    {:else}
      <div class="complete"><Check size={18}/><span>All simulated checks passed</span></div><button class="primary" onclick={close}>Done</button>
    {/if}
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;padding:20px;background:rgba(0,0,0,.65);backdrop-filter:blur(8px)}.modal{position:relative;width:min(430px,100%);padding:25px;border:1px solid var(--border-strong);border-radius:18px;background:#171816;box-shadow:0 30px 90px rgba(0,0,0,.6)}.close{position:absolute;top:14px;right:14px;display:grid;width:29px;height:29px;place-items:center;border:1px solid var(--border);border-radius:8px;background:var(--surface-deep);color:var(--text-dim);cursor:pointer}.modal-icon{display:grid;width:42px;height:42px;place-items:center;border-radius:12px;background:color-mix(in srgb,var(--accent) 12%,transparent);color:var(--accent)}.eyebrow{margin:17px 0 5px;color:var(--accent);font-size:9px;font-weight:700;letter-spacing:.1em;text-transform:uppercase}h2{margin:0;color:var(--text);font-size:21px;letter-spacing:-.03em}.intro{margin:8px 0 17px;color:var(--text-dim);font-size:10px;line-height:1.5}.digest{display:flex;align-items:center;justify-content:space-between;padding:9px 11px;border:1px solid var(--border);border-radius:8px;background:var(--surface-deep)}.digest span{color:var(--text-dim);font-size:8px;text-transform:uppercase}.digest code{color:var(--text-muted);font-size:9px}ol{display:grid;gap:0;margin:13px 0;padding:0;list-style:none}li{display:grid;grid-template-columns:25px 1fr;gap:10px;padding:10px 0;border-bottom:1px solid var(--border)}li>span{display:grid;width:21px;height:21px;place-items:center;border:1px solid var(--border-strong);border-radius:50%;color:var(--text-dim);font-size:8px}li div{display:flex;flex-direction:column;gap:3px}li strong{color:var(--text-muted);font-size:10px}li small{color:var(--text-dim);font-size:8px}.safe{display:flex;gap:9px;margin:13px 0;padding:10px;border-radius:8px;background:rgba(83,190,130,.07);color:var(--success)}.safe span{display:flex;flex-direction:column;gap:2px}.safe strong{font-size:9px}.safe small{color:var(--text-dim);font-size:8px}.primary{width:100%;min-height:39px;border:1px solid #f0c74f;border-radius:9px;background:var(--accent);color:#17140a;font:inherit;font-size:11px;font-weight:750;cursor:pointer}.complete{display:flex;align-items:center;gap:9px;margin:24px 0;padding:13px;border-radius:9px;background:rgba(83,190,130,.08);color:var(--success);font-size:10px;font-weight:650}
</style>
