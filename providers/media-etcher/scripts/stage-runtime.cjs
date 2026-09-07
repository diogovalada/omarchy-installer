// Qualify a relocated Node + node_modules directory. No single-executable claim.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const stageRoot = path.join(root, '.qualification-runtime');
fs.mkdirSync(stageRoot, { recursive: true });
const stage = fs.mkdtempSync(path.join(stageRoot, `${process.platform}-${process.arch}-`));
const executable = process.platform === 'win32' ? 'node.exe' : 'node';
fs.copyFileSync(process.execPath, path.join(stage, executable));
fs.cpSync(path.join(root, 'dist'), path.join(stage, 'dist'), { recursive: true });
fs.cpSync(path.join(root, 'node_modules'), path.join(stage, 'node_modules'), { recursive: true });
fs.copyFileSync(path.join(__dirname, 'runtime-smoke.cjs'), path.join(stage, 'runtime-smoke.cjs'));
const result = spawnSync(path.join(stage, executable), ['runtime-smoke.cjs'], {
  cwd: stage, windowsHide: true, encoding: 'utf8', timeout: 60000,
  // Prevent a developer's module search path from satisfying missing staged dependencies.
  env: { ...process.env, NODE_PATH: '', NODE_OPTIONS: '' },
});
process.stdout.write(result.stdout ?? '');
process.stderr.write(result.stderr ?? '');
if (result.error) console.error(result.error.message);
console.log(JSON.stringify({ stage, exitCode: result.status }));
process.exitCode = result.status === 0 ? 0 : 1;
