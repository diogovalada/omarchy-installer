<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { HardDriveDownload, Usb } from 'lucide-svelte';
  import OmarchyWordmark from './lib/OmarchyWordmark.svelte';
  import DownloadPanel from './lib/DownloadPanel.svelte';
  import SetupPanel from './lib/SetupPanel.svelte';
  import AppleSetupPanel from './lib/AppleSetupPanel.svelte';
  import { appleSetup } from './lib/appleSetup';
  import { downloads } from './lib/downloads';
  import { setup, setupActive } from './lib/setup';
  let option: 'usb' | 'direct' | null = null;
  let usbButton: HTMLButtonElement;
  let directButton: HTMLButtonElement;
  async function closeOption() {
    const previous=option;
    option=null;
    await tick();
    (previous==='usb' ? usbButton : directButton)?.focus();
  }
  $: appleHost = $downloads.snapshot?.host_os === 'macos' && $downloads.snapshot?.host_architecture === 'aarch64';
  $: if (!option && setupActive($setup.snapshot?.status)) option=$setup.snapshot?.kind ?? null;
  onMount(() => {
    void setup.refresh();
    const timer = setInterval(() => { void downloads.refresh(); void setup.refresh(); if (appleHost) void appleSetup.refresh(); }, 1000);
    return () => clearInterval(timer);
  });
</script>

<main class="setup">
  <div class="setup-content">
    <header><div class="wordmark"><OmarchyWordmark/></div><span>Setup <span class="edition">/ Community edition</span></span></header>
    <DownloadPanel compact={option !== null} locked={setupActive($setup.snapshot?.status) || $setup.pending}/>
    {#if option==='direct' && appleHost}<AppleSetupPanel close={closeOption}/>{:else if option}<SetupPanel kind={option} close={closeOption}/>{:else}
      <section class="next-actions" aria-label="Installation options">
        <button bind:this={usbButton} aria-describedby="usb-option-description" disabled={setupActive($setup.snapshot?.status) || $setup.pending} onclick={() => { option='usb'; }}><Usb size={21}/><span>Create bootable USB</span></button>
        <button bind:this={directButton} aria-describedby="direct-option-description" disabled={setupActive($setup.snapshot?.status) || $setup.pending} onclick={() => { option='direct'; }}><HardDriveDownload size={21}/><span>Install without USB</span></button>
        <p id="usb-option-description">Make an installer for an x86-64 PC.</p>
        <p id="direct-option-description">Install on this computer.</p>
      </section>
      <p class="availability">{$downloads.snapshot?.status === 'complete' && $downloads.snapshot.image_path ? 'Your image is ready. Choose how to install Omarchy.' : 'Choose an option to check image and disk requirements.'}</p>
    {/if}
    <footer>{#if !downloads.native}Browser preview · downloads require the desktop app{:else if $downloads.error && !$downloads.snapshot}Desktop connection unavailable{:else}v0.1.0 · {$downloads.snapshot?.host_os ?? 'Connecting…'}{/if}</footer>
  </div>
</main>

<style>
  .setup{height:100dvh;overflow-y:auto;padding:40px 24px;scrollbar-color:var(--border-strong) transparent}
  .setup-content{width:100%;max-width:760px;margin:0 auto}
  header{display:flex;align-items:center;justify-content:space-between;gap:24px;margin-bottom:30px}
  .wordmark{width:190px;flex-shrink:0}header>span{color:var(--text-dim);font-size:11px}
  .next-actions{display:grid;grid-template-columns:1fr 1fr;gap:16px;margin-top:24px}
  .next-actions button{display:flex;align-items:center;justify-content:center;gap:12px;min-height:76px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--text);font-size:14px;cursor:pointer}
  .next-actions button:hover:not(:disabled){border-color:var(--accent);color:var(--accent)}
  .next-actions button:disabled{opacity:.65}
  .next-actions p{margin:-6px 0 0;text-align:center;color:var(--text-muted);font-size:11px;line-height:1.7}
  .availability{margin:12px 0 0;color:var(--text-dim);font-size:10px;text-align:center;line-height:1.7}
  footer{margin-top:28px;color:var(--text-dim);font-size:10px;text-align:center;line-height:1.7}
  @media(max-width:550px){.setup{padding:24px 16px}header{margin-bottom:24px;gap:16px}.wordmark{width:165px}.edition{display:none}.next-actions{gap:12px;margin-top:20px}.next-actions button{font-size:12px;gap:9px;min-height:66px}footer{font-size:9px}}
</style>
