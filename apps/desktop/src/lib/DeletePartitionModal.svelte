<script lang="ts">
  import { onMount } from 'svelte';
  import { TriangleAlert } from 'lucide-svelte';
  import { formatBytes } from './downloads';
  import type { StorageChoice } from './storageChoices';
  export let choice: StorageChoice;
  export let confirm: (identifier:string) => void;
  export let close: () => void;
  let dialog: HTMLDialogElement;
  let keepButton: HTMLButtonElement;
  let typed = '';
  $: deletion = choice.deletion;
  $: subject = deletion?.scope==='installation' ? 'installation' : 'partition';
  $: matches = !!deletion && typed.trim().toLowerCase() === deletion.identifier.toLowerCase();
  onMount(() => { dialog.showModal(); keepButton.focus(); return () => dialog.close(); });
  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (matches) confirm(typed.trim());
  }
</script>

<dialog bind:this={dialog} aria-labelledby="delete-title" aria-describedby="delete-warning" oncancel={close}>
  <form onsubmit={submit}>
    <div class="warning-icon"><TriangleAlert size={24}/></div>
    <h2 id="delete-title">Delete this {subject} and its data?</h2>
    <p id="delete-warning" class="warning">All files and any operating system in this {subject} will be lost. The installer cannot undo this.</p>
    <p>Continue only if you have checked the {subject} and backed up anything you need.</p>
    <dl>
      <div><dt>Target</dt><dd>{choice.label}</dd></div>
      <div><dt>Identifier</dt><dd>{deletion?.identifier}</dd></div>
      <div><dt>Name</dt><dd>{deletion?.partitionLabel || 'No name'}</dd></div>
      {#if deletion?.fileSystem}<div><dt>Filesystem</dt><dd>{deletion.fileSystem}</dd></div>{/if}
      <div><dt>Entire {subject}</dt><dd>{deletion?.sizeDescription ?? formatBytes(deletion?.sizeBytes ?? 0)}</dd></div>
    </dl>
    <p>Deletion is added to your installation plan. Nothing is deleted now; you will confirm the final plan before disk changes begin.</p>
    <label for="delete-confirmation">Type <strong>{deletion?.identifier}</strong> to continue.</label>
    <input id="delete-confirmation" type="text" bind:value={typed} autocomplete="off" autocapitalize="off" spellcheck="false" aria-describedby="delete-input-hint"/>
    <small id="delete-input-hint">Use the identifier shown above. Capitalization does not matter.</small>
    <div class="actions">
      <button bind:this={keepButton} type="button" onclick={close}>Keep {subject}</button>
      <button class="danger" type="submit" disabled={!matches}>Queue deletion and prepare</button>
    </div>
  </form>
</dialog>

<style>
  dialog{width:min(510px,calc(100vw - 40px));max-height:calc(100vh - 40px);box-sizing:border-box;overflow:auto;padding:28px;border:1px solid var(--border-strong);border-radius:6px;background:var(--surface);color:var(--text);font-family:inherit;box-shadow:0 24px 80px #0008}
  dialog::backdrop{background:#000b}.warning-icon{color:var(--error)}h2{margin:14px 0;font-size:17px;font-weight:400;line-height:1.5}p{font-size:11px;line-height:1.8;color:var(--text-muted)}p.warning{color:var(--error)}
  dl{margin:20px 0;font-size:11px;line-height:1.7}dl>div{display:flex;justify-content:space-between;gap:20px;padding:7px 0;border-bottom:1px solid var(--border)}dt{color:var(--text-dim)}dd{margin:0;max-width:70%;text-align:right;overflow-wrap:anywhere}
  label{display:block;margin-top:20px;font-size:11px;line-height:1.8}label strong{font-weight:700}input{display:block;box-sizing:border-box;width:100%;margin-top:9px;padding:11px;border:1px solid var(--border-strong);border-radius:3px;background:var(--surface-deep);color:var(--text);font:inherit;font-size:12px}small{display:block;margin-top:8px;color:var(--text-dim);font-size:10px;line-height:1.7}
  .actions{display:flex;justify-content:flex-end;gap:12px;flex-wrap:wrap;margin-top:24px}button{padding:10px 14px;min-height:40px;border:1px solid var(--border-strong);border-radius:4px;background:var(--surface-raised);color:var(--text);font:inherit;font-size:11px;cursor:pointer}button.danger{border-color:var(--error);background:var(--error);color:var(--surface-deep)}button:disabled{opacity:.4;cursor:not-allowed}button:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:3px}
</style>
