<script lang="ts">
  import { Check, Cpu, HardDrive, MemoryStick, Play, ShieldCheck } from 'lucide-svelte';
  import type { HostProfile } from '../lib/model';
  import FlowHeader from '../lib/FlowHeader.svelte';
  import StatusPill from '../lib/StatusPill.svelte';
  import ActionButton from '../lib/ActionButton.svelte';
  export let host: HostProfile;
  export let openPlan: (title: string) => void;
  let persistence = true;
  let ram = '8 GB';
</script>

<div class="flow-page">
  <FlowHeader eyebrow="Try" title="A room of its own." description="Run Omarchy in an isolated virtual machine. Your current operating system, files, and boot settings stay untouched." />
  <div class="two-column">
    <section class="panel feature">
      <div class="panel-head"><div class="provider"><span class="hero-icon"><Play size={21} fill="currentColor" /></span><div><h2>Omarchy virtual machine</h2><p>{host.label} provider</p></div></div><StatusPill status={host.tryStatus} /></div>
      <div class="screen"><div class="screen-bar"><i></i><i></i><i></i><span>Omarchy</span></div><div class="screen-content"><span>OMARCHY</span><small>Hyprland · Arch Linux</small></div></div>
      <ul><li><Check size={14} /> Downloads from a verified release</li><li><Check size={14} /> Runs inside a platform-native VM</li><li><Check size={14} /> Easy to delete when you are finished</li></ul>
    </section>
    <section class="panel settings">
      <h2>Virtual machine settings</h2><p class="lead">Comfortable defaults based on the simulated host.</p>
      <label><span><MemoryStick size={15} /> Memory</span><select bind:value={ram}><option>4 GB</option><option>8 GB</option><option>12 GB</option></select></label>
      <label><span><Cpu size={15} /> Processors</span><strong>4 cores</strong></label>
      <label><span><HardDrive size={15} /> Storage</span><strong>64 GB, dynamic</strong></label>
      <button class="toggle-row" role="switch" aria-checked={persistence} onclick={() => persistence = !persistence}><span><strong>Keep my changes</strong><small>Preserve apps and files between sessions</small></span><i class:on={persistence}><b></b></i></button>
      <div class="assurance"><ShieldCheck size={16} /><span><strong>Isolated by default</strong><small>Clipboard and shared folders start disabled.</small></span></div>
      <ActionButton label="Review simulated launch" disabled={host.tryStatus === 'unavailable'} onClick={() => openPlan('Virtual machine launch')} />
    </section>
  </div>
</div>

<style>
  .two-column { display:grid; grid-template-columns: 1.2fr .8fr; gap:14px; margin-top:30px; }
  .panel { padding:20px; border:1px solid var(--border); border-radius:15px; background:var(--surface); }
  .panel-head,.provider { display:flex; align-items:center; justify-content:space-between; gap:12px; }.provider { justify-content:flex-start; }.hero-icon{display:grid;width:40px;height:40px;place-items:center;border-radius:11px;background:color-mix(in srgb,var(--accent) 12%,transparent);color:var(--accent)}
  h2{margin:0;color:var(--text);font-size:14px}.provider p,.lead{margin:4px 0 0;color:var(--text-dim);font-size:11px}.screen{overflow:hidden;margin-top:20px;border:1px solid var(--border-strong);border-radius:11px;background:#080909;box-shadow:0 16px 40px rgba(0,0,0,.23)}.screen-bar{display:flex;align-items:center;gap:5px;height:26px;padding:0 9px;background:#171817}.screen-bar i{width:6px;height:6px;border-radius:50%;background:#393b38}.screen-bar i:first-child{background:#d3ad36}.screen-bar span{margin:auto;color:#777;font-size:8px}.screen-content{display:flex;min-height:153px;align-items:center;justify-content:center;flex-direction:column;background:radial-gradient(circle at center,#232820,#080909 65%)}.screen-content span{color:#e9c858;font-size:25px;font-weight:780;letter-spacing:.22em}.screen-content small{margin-top:9px;color:#767971;font-size:8px;letter-spacing:.12em}ul{display:grid;gap:9px;margin:17px 0 0;padding:0;list-style:none}li{display:flex;align-items:center;gap:8px;color:var(--text-muted);font-size:11px}li svg{color:var(--success)}
  .settings{display:flex;flex-direction:column}.settings label{display:flex;align-items:center;justify-content:space-between;min-height:46px;border-bottom:1px solid var(--border);color:var(--text-muted);font-size:11px}.settings label span{display:flex;align-items:center;gap:8px}.settings label svg{color:var(--text-dim)}select{padding:5px 7px;border:1px solid var(--border);border-radius:6px;background:var(--surface-deep);color:var(--text);font:inherit;font-size:11px}label strong{color:var(--text);font-weight:600}.toggle-row{display:flex;align-items:center;justify-content:space-between;width:100%;padding:14px 0;border:0;border-bottom:1px solid var(--border);background:none;color:inherit;text-align:left;cursor:pointer}.toggle-row span{display:flex;flex-direction:column;gap:3px}.toggle-row strong{font-size:11px}.toggle-row small{color:var(--text-dim);font-size:10px}.toggle-row i{position:relative;width:30px;height:17px;border-radius:12px;background:#3b3c38}.toggle-row i b{position:absolute;top:3px;left:3px;width:11px;height:11px;border-radius:50%;background:#8b8c87;transition:.18s}.toggle-row i.on{background:var(--accent)}.toggle-row i.on b{left:16px;background:#17140a}.assurance{display:flex;gap:9px;margin:16px 0;padding:11px;border-radius:9px;background:rgba(83,190,130,.07);color:var(--success)}.assurance span{display:flex;flex-direction:column;gap:2px}.assurance strong{font-size:10px}.assurance small{color:var(--text-dim);font-size:9px}.settings :global(button:last-child){margin-top:auto}
  @media(max-width:800px){.two-column{grid-template-columns:1fr}}
</style>
