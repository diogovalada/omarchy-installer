<script lang="ts">
  import FlowHeader from '../lib/FlowHeader.svelte';
  import DownloadPanel from '../lib/DownloadPanel.svelte';
</script>
<div class="flow-page">
  <FlowHeader eyebrow="Download" title="Keep a verified image." description="Download the official Omarchy installation image to your computer. Interrupted downloads can resume from the local cache."/>
  <DownloadPanel/>
</div>
