<script lang="ts">
  export let defaultOs:'omarchy'|'windows'='omarchy';
  export let timeoutSeconds=5;
</script>

<fieldset>
  <legend>When this PC starts</legend>
  <p>A menu will let you choose Omarchy or Windows at each normal startup.</p>
  <div class="settings">
    <label>Start automatically<select bind:value={defaultOs}><option value="omarchy">Omarchy (recommended)</option><option value="windows">Windows</option></select></label>
    <label>Wait before starting<select bind:value={timeoutSeconds}>{#each [5,10,15,30] as seconds}<option value={seconds}>{seconds} seconds</option>{/each}</select></label>
  </div>
  <p class="preview">Start {defaultOs==='omarchy' ? 'Omarchy' : 'Windows'} automatically after {timeoutSeconds} seconds.</p>
  <p>Use the keyboard to stop the countdown and choose an OS for this startup. Your saved default stays the same. Choosing Windows triggers a brief extra restart before Windows starts.</p>
</fieldset>

<style>
  fieldset{min-width:0;margin:24px 0 0;padding:18px;border:1px solid var(--border);border-radius:4px}legend{font-size:12px;color:var(--text)}p{font-size:10px;line-height:1.8;color:var(--text-dim);margin:10px 0}.preview{color:var(--text-muted)}.settings{display:flex;gap:18px;flex-wrap:wrap}label{display:grid;gap:8px;font-size:11px;color:var(--text-muted)}select{padding:9px 12px;min-height:38px;max-width:100%;border:1px solid var(--border-strong);border-radius:3px;background:var(--surface-deep);color:var(--text);font:inherit}select:focus-visible{outline:2px solid var(--accent);outline-offset:3px}
</style>
