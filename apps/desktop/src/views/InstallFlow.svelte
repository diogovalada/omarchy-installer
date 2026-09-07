<script lang="ts">
  import FlowHeader from '../lib/FlowHeader.svelte';
  import ActionButton from '../lib/ActionButton.svelte';
  import windingRoad from '../assets/omarchy/winding-road.webp';
  export let openDownload: () => void;
</script>
<div class="flow-page">
  <FlowHeader eyebrow="Install" title="A new home for your computer." description="Direct installation is under development. This build cannot assess compatibility or make changes to your system."/>
  <section>
    <img src={windingRoad} alt="A winding road through a green landscape, from Omarchy’s Tokyo Night wallpapers"/>
    <div class="details"><span class="label">THE NEXT CHAPTER</span><h2>Installation is not available yet</h2><p>You can download the official x86-64 ISO while direct installation is being developed.</p><ActionButton label="Download image" onClick={openDownload}/></div>
  </section>
</div>
<style>
  section{display:grid;grid-template-columns:1fr 1fr;margin-top:28px;border:1px solid var(--border);border-radius:4px;background:var(--surface);overflow:hidden}
  section>img{display:block;width:100%;height:100%;min-height:300px;object-fit:cover;object-position:center}
  .details{padding:28px}.label{color:var(--accent);font-size:9px;letter-spacing:.1em}h2{margin:18px 0 0;font-size:17px;font-weight:400;line-height:1.6}p{margin:14px 0 24px;color:var(--text-muted);font-size:11px;line-height:1.9}
  @media(max-width:760px){section{grid-template-columns:1fr}section>img{min-height:0;max-height:230px;aspect-ratio:16/9}.details{padding:24px}h2{font-size:18px}}
</style>
