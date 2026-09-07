<script lang="ts">
  import { ArrowUpRight, Download, HardDriveDownload, Usb } from 'lucide-svelte';
  import OmarchyWordmark from '../lib/OmarchyWordmark.svelte';
  import quattro from '../assets/omarchy/quattro.webp';
  import { downloads } from '../lib/downloads';
  export let navigate: (flow: 'overview' | 'install' | 'usb' | 'download') => void;
  $: actions = [
    { id: 'download' as const, title: 'Download image', desc: 'The official ISO, verified and saved to your computer.', icon: Download, status: downloads.native ? 'Ready to download' : 'Desktop app required' },
    { id: 'usb' as const, title: 'Create USB', desc: 'Prepare your image for a bootable installation drive.', icon: Usb, status: 'Image preparation only' },
    { id: 'install' as const, title: 'Install', desc: 'Make room for Omarchy on this computer.', icon: HardDriveDownload, status: 'Coming soon' }
  ];
</script>
<div class="flow-page overview">
  <section class="welcome" aria-label="Welcome to Omarchy">
    <div class="intro">
      <p class="eyebrow">Make yourself at home.</p>
      <div class="wordmark"><OmarchyWordmark/></div>
      <h1>Make this computer<br/>your own.</h1>
      <p class="intro-copy">Start with the official image.<br/>We’ll take care of verification.</p>
    </div>
    <figure>
      <img src={quattro} alt="Omarchy’s Quattro wallpaper: a rally car against a vivid pink and orange sunset"/>
      <figcaption><span>OMARCHY / QUATTRO</span><span>Made to be yours.</span></figcaption>
    </figure>
  </section>
  <div class="section-heading"><h2>Choose your path</h2><span>LET’S GET STARTED</span></div>
  <section class="action-grid" aria-label="Setup actions">
    {#each actions as action, index}
      <button class:primary={action.id === 'download'} onclick={() => navigate(action.id)}>
        <div class="card-top"><svelte:component this={action.icon} size={22} strokeWidth={1.8}/><span>0{index + 1}</span></div>
        <h3>{action.title}<ArrowUpRight size={17}/></h3>
        <p>{action.desc}</p>
        <span class="status">{action.id === 'usb' && !downloads.native ? 'Desktop app required' : action.status}</span>
      </button>
    {/each}
  </section>
  <p class="note">For x86-64 PCs and compatible Intel Macs.<br/>Apple silicon will use a separate native installer.</p>
</div>
<style>
  .welcome{display:grid;grid-template-columns:.95fr 1.05fr;align-items:center;gap:28px;margin:2px 0 24px}
  .eyebrow{margin:0 0 18px;color:var(--accent);font-size:11px}
  .wordmark{width:100%;max-width:335px}
  h1{margin:22px 0 12px;font-size:clamp(19px,2.1vw,26px);font-weight:400;line-height:1.4;letter-spacing:-.05em}
  .intro-copy{margin:0;color:var(--text-muted);font-size:11px;line-height:1.9}
  figure{margin:0;min-width:0;border:1px solid var(--border-strong);background:var(--surface-deep)}
  figure img{display:block;width:100%;aspect-ratio:16/9;object-fit:contain}
  figcaption{display:flex;justify-content:space-between;gap:10px;padding:11px 12px;color:var(--text-muted);font-size:8px;line-height:1.5}
  figcaption span:first-child{color:var(--accent)}
  .section-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:16px;padding-top:21px;border-top:1px solid var(--border)}
  .section-heading h2{margin:0;font-size:14px;font-weight:400}.section-heading>span{font-size:9px;letter-spacing:.08em;color:var(--text-dim)}
  .action-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px}
  .action-grid button{display:flex;flex-direction:column;min-height:213px;padding:19px 17px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--text);text-align:left;cursor:pointer;transition:background .15s,border-color .15s}
  .action-grid button.primary{border-color:var(--accent);background:var(--surface-raised)}
  .action-grid button:hover{border-color:var(--accent-hover);background:var(--surface-hover)}
  .card-top{display:flex;align-items:center;justify-content:space-between;color:var(--accent)}.card-top>span{font-size:10px;color:var(--text-dim)}
  h3{display:flex;align-items:center;justify-content:space-between;gap:8px;margin:21px 0 10px;font-size:13px;font-weight:700}h3 :global(svg){flex-shrink:0;color:var(--accent)}
  .action-grid p{margin:0 0 20px;color:var(--text-muted);font-size:10px;line-height:1.8}
  .status{display:block;margin-top:auto;padding-top:12px;border-top:1px solid var(--border);color:var(--text-dim);font-size:9px;line-height:1.6}.primary .status{color:var(--accent)}
  .note{margin:20px 0 0;color:var(--text-dim);font-size:9px;line-height:1.9}
  @media(max-width:1000px){.welcome{gap:20px}.intro-copy{font-size:10px}figcaption span:last-child{display:none}.action-grid{gap:9px}.action-grid button{padding:16px 13px}h3{font-size:12px}}
  @media(max-width:760px){.welcome{grid-template-columns:1fr;margin-bottom:26px;gap:26px}.intro{max-width:440px}.wordmark{max-width:320px}h1{font-size:25px}.intro-copy{font-size:12px}.eyebrow{font-size:11px}figure{max-width:600px}figcaption{font-size:9px}figcaption span:last-child{display:block}.action-grid{grid-template-columns:1fr}.action-grid button{min-height:180px;padding:20px}.card-top>span{font-size:11px}h3{font-size:15px;margin-top:17px}.action-grid p{font-size:12px}.status{font-size:10px}.note{font-size:10px}.section-heading>span{font-size:8px}}
</style>
