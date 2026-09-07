// Read-only timing: hashes packaged files and runs only probe/list commands.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { performance } = require('node:perf_hooks');
const [manifestPath, providerRoot, sourcePath, mode = 'baseline'] = process.argv.slice(2);
if (!['baseline', 'combined'].includes(mode)) throw new Error('Unsupported profile mode');
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const runtime = mode === 'baseline' ? manifest.media : manifest.media_inspection;
const inspectionPrefix = runtime.entrypoint.split('/inspection/')[0] + '/inspection/';
const verify = files => {
  const start = performance.now();
  let bytes = 0;
  for (const file of files) {
    const data = fs.readFileSync(path.join(providerRoot, file.path));
    if (data.length !== file.length || createHash('sha256').update(data).digest('hex') !== file.sha256) throw new Error('Provider hash mismatch');
    bytes += data.length;
  }
  return { seconds: (performance.now() - start) / 1000, files: files.length, bytes };
};
const command = action => {
  const start = performance.now();
  const result = spawnSync(path.join(providerRoot, runtime.executable), [path.join(providerRoot, runtime.entrypoint)], {
    input: JSON.stringify({ protocol: 1, action, ...(action === 'list' ? { sourcePath } : {}) }) + '\n',
    encoding: 'utf8', windowsHide: true, timeout: 120000, maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  const terminal = result.stdout.trim().split(/\r?\n/).map(line => JSON.parse(line)).at(-1);
  if (result.status !== 0 || terminal?.type !== 'result') throw new Error(terminal?.message ?? result.stderr);
  if (action === 'probe' && !terminal.result.available) throw new Error(terminal.result.reason);
  return { seconds: (performance.now() - start) / 1000, ...(action === 'list' ? { drives: terminal.result.drives.length } : {}) };
};
const result = { mode, verification: verify(mode === 'baseline' ? manifest.files : manifest.files.filter(file => file.path === runtime.executable || file.path.startsWith(inspectionPrefix))) };
if (mode === 'baseline') result.probe = command('probe');
result.list = command('list');
result.totalSeconds = result.verification.seconds + (result.probe?.seconds ?? 0) + result.list.seconds;
console.log(JSON.stringify(result, null, 2));
