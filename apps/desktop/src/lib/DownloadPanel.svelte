<script lang="ts">
  import { onMount } from 'svelte';
  import { Check, Download, FolderOpen } from 'lucide-svelte';
  import quattro from '../assets/omarchy/quattro.webp';
  import { downloads, formatBytes, isActive } from './downloads';
  export let compact = false;
  export let locked = false;
  let expanded = false;
  $: snapshot = $downloads.snapshot;
  $: release = snapshot?.release;
  $: active = isActive(snapshot?.status) || $downloads.pending;
  $: complete = snapshot?.status === 'complete' && !!snapshot.image_path;
  $: percent = snapshot?.total_bytes ? Math.min(100, snapshot.received_bytes / snapshot.total_bytes * 100) : 0;
  $: transfer = !!snapshot && ['preparing', 'downloading', 'verifying', 'saving'].includes(snapshot.status);
  $: indeterminate = !!snapshot && ['resolving', 'preparing', 'saving'].includes(snapshot.status);
  $: statusText = complete ? 'Verified' : ({ idle: 'Checking release…', resolving: 'Checking release…', ready: snapshot?.existing_image ? 'Image found · verification required' : 'Ready to download', preparing: 'Preparing…', downloading: 'Downloading · ' + Math.floor(percent) + '%', verifying: 'Verifying · ' + Math.floor(percent) + '%', saving: 'Saving…', complete: 'Image unavailable', cancelled: 'Paused', failed: snapshot?.existing_image ? 'Verification failed' : 'Download failed' })[snapshot?.status ?? 'idle'];
  onMount(() => { void downloads.refresh().then(() => { if ($downloads.snapshot?.status === 'idle') void downloads.resolve(); }); });
</script>

<section class="download-panel" class:compact={compact && complete} aria-label="Omarchy image download">
  {#if compact && complete}
    <div class="image-summary"><span role="status"><Check size={17}/>Omarchy {release?.version} · Verified</span><button class="text-button" aria-expanded={expanded} onclick={()=>{expanded=!expanded;}}>{expanded ? 'Hide image details' : 'Image details'}</button></div>
  {/if}
  {#if !compact || !complete || expanded}
  <div class="release">
    <div><h1>Omarchy {release?.version ?? 'ISO'}</h1><p>x86-64{#if release}<span> · {formatBytes(release.length)}</span>{/if}</p></div>
    <img src={quattro} alt="Omarchy Quattro wallpaper"/>
  </div>
  <div class="download-body">
    <div class="destination"><FolderOpen size={17}/><div><span class="sr-only">Save to </span><span title={snapshot?.destination_directory}>{snapshot?.destination_directory ?? 'Downloads/Omarchy'}</span></div><button class="text-button" disabled={!downloads.native || active || locked} onclick={() => { void downloads.chooseDirectory(); }}>Change</button></div>
    <div class="progress-label"><span class:verified={complete} role="status" aria-live="polite">{#if complete}<Check size={17}/>{/if}{downloads.native ? (snapshot?.cancel_requested && active ? 'Stopping…' : statusText) : 'Not downloaded'}</span>{#if snapshot?.status === 'downloading'}<span class="bytes">{formatBytes(snapshot.received_bytes)} / {formatBytes(snapshot.total_bytes)}</span>{/if}</div>
    {#if indeterminate}<progress max="100" aria-label={statusText}></progress>{:else}<progress max="100" value={complete ? 100 : percent} aria-label="Download progress" class:verified={complete}></progress>{/if}
    {#if $downloads.error || (snapshot?.error && snapshot.status !== 'cancelled')}<p class="error" role="alert">{$downloads.error ?? snapshot?.error}</p>{/if}
    <div class="controls">
      {#if transfer}<button class="secondary" disabled={$downloads.pending || snapshot?.cancel_requested} onclick={() => { void downloads.cancel(); }}>{snapshot?.status === 'downloading' ? 'Pause' : 'Cancel'}</button>
      {:else if !complete}<button class="primary" disabled={!downloads.native || active || locked} onclick={() => { if (release) void downloads.start(); else void downloads.resolve(); }}><Download size={16}/>{release ? (snapshot?.existing_image ? 'Verify' : snapshot?.status === 'cancelled' || snapshot?.status === 'failed' ? 'Resume' : 'Download') : (snapshot?.status === 'failed' || $downloads.error ? 'Retry' : 'Download')}</button>{/if}
      {#if release}<details><summary>Details</summary><div class="detail-content"><p>{release.file_name}</p>{#if complete}<p>File size, SHA-256 and signature verified.</p>{:else}<p>File size, SHA-256 and signature are checked before use.</p>{/if}{#if snapshot?.image_locked}<p>The ISO is kept read-only so installation can reuse this verification. Close the app to move or edit the file.</p>{/if}<p>SHA-256 <code>{release.sha256}</code></p><p>Signing key <code>{release.signer_fingerprint}</code></p>{#if snapshot?.image_path}<p>Saved to <code>{snapshot.image_path}</code></p>{/if}<button class="text-button" disabled={active || locked} onclick={() => { void downloads.resolve(); }}>Check for updates</button></div></details>{/if}
    </div>
  </div>
  {/if}
</section>

<style>
  .download-panel{border:1px solid var(--border);border-radius:4px;background:var(--surface);overflow:hidden}
  .image-summary{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:14px 24px}.image-summary>span{display:flex;align-items:center;gap:9px;color:var(--success);font-size:12px}.compact .release{border-top:1px solid var(--border)}
  .release{display:flex;align-items:center;justify-content:space-between;gap:24px;padding:24px;border-bottom:1px solid var(--border)}
  h1{margin:0;font-size:22px;font-weight:400;line-height:1.4}.release p{margin:10px 0 0;color:var(--text-dim);font-size:11px}
  .release img{display:block;width:190px;aspect-ratio:16/9;object-fit:contain;border-radius:2px}
  .download-body{padding:24px}.destination{display:flex;align-items:center;gap:10px;color:var(--text-muted);font-size:11px}.destination :global(svg){flex-shrink:0}.destination>div{min-width:0;flex:1;overflow-wrap:anywhere}
  button{display:inline-flex;align-items:center;justify-content:center;gap:9px;min-height:40px;padding:10px 16px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font-size:12px;cursor:pointer}
  button:hover:not(:disabled){border-color:var(--accent)}button:disabled{opacity:.5;cursor:not-allowed}.text-button{min-height:32px;padding:6px 0;border:0;background:none;color:var(--accent);font-size:11px}.primary{border-color:var(--accent);background:var(--accent);color:var(--surface-deep);font-weight:700;min-width:130px}.primary:hover:not(:disabled){background:var(--accent-hover)}
  .progress-label{display:flex;justify-content:space-between;align-items:center;gap:12px;margin-top:24px;font-size:11px;color:var(--text-muted)}.progress-label>span:first-child{display:flex;align-items:center;gap:7px;min-height:20px}.bytes{font-size:10px;color:var(--text-dim)}
  progress{display:block;width:100%;height:6px;margin:12px 0 22px;border:0;border-radius:0;accent-color:var(--accent);background:var(--surface-deep)}progress::-webkit-progress-bar{background:var(--surface-deep)}progress::-webkit-progress-value{background:var(--accent)}progress.verified::-webkit-progress-value{background:var(--success)}progress.verified{accent-color:var(--success)}.progress-label .verified{color:var(--success)}
  .controls{display:flex;align-items:flex-start;gap:20px;flex-wrap:wrap;min-height:40px}.controls>details{flex:1;min-width:140px;padding-top:11px}summary{width:fit-content;color:var(--text-dim);font-size:11px;cursor:pointer}.detail-content{padding-top:12px}.detail-content p{font-size:10px;line-height:1.8;color:var(--text-muted);overflow-wrap:anywhere}code{font:inherit;display:block;color:var(--text-dim)}
  .error{font-size:11px;line-height:1.7;color:var(--error);overflow-wrap:anywhere;margin:0 0 18px}
  @media(max-width:550px){.release{padding:20px;gap:16px}.release img{width:125px}h1{font-size:18px}.release p{font-size:10px}.download-body{padding:20px}.progress-label{flex-wrap:wrap;gap:4px}.destination{font-size:10px}.controls:has(details[open]){flex-direction:column}.controls>details{width:100%}}
  @media(max-width:370px){.release img{width:95px}h1{font-size:16px}}
</style>
