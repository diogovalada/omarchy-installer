<script lang="ts">
  import FlowHeader from '../lib/FlowHeader.svelte';
  import DownloadPanel from '../lib/DownloadPanel.svelte';
  import ActionButton from '../lib/ActionButton.svelte';
</script>
<div class="flow-page">
  <FlowHeader eyebrow="Create USB" title="Prepare your installation image." description="USB creation will use the same verified download as the download page. You can prepare the image here now."/>
  <DownloadPanel/>
  <section class="unavailable"><h2>USB writing is not available in this build</h2><p>Drive selection and writing will be enabled after the native USB integration is tested.</p><ActionButton label="Choose USB drive" disabled/></section>
</div>
<style>.unavailable{margin-top:24px;padding:20px;border:1px solid var(--border);border-radius:4px;background:var(--surface)}h2{margin:0;font-size:13px}p{margin:9px 0 18px;color:var(--text-muted);font-size:11px;line-height:1.85}</style>
