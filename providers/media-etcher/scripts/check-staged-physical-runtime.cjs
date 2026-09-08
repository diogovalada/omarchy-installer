// Package and load native modules without enumerating, opening or writing disks.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const temporaryRoot = fs.realpathSync(os.tmpdir());
const directory = fs.mkdtempSync(path.join(temporaryRoot, 'omarchy-media-staging-'));
const output = path.join(directory, 'runtime');
try {
  execFileSync(process.execPath, [path.join(__dirname, 'stage-physical-runtime.cjs'), '--output', output], { stdio: 'inherit' });
  const manifest = JSON.parse(fs.readFileSync(path.join(output, 'runtime-manifest.json'), 'utf8'));
  const executable = path.join(output, manifest.executable);
  execFileSync(executable, ['-e', 'require(process.argv[1])', path.join(output, 'dist/physical-engine.js')], { stdio: 'inherit' });
  const isolated = path.join(directory, 'isolated-inspection');
  fs.renameSync(path.join(output, 'inspection'), isolated);
  assert.equal(fs.existsSync(path.join(isolated, 'dist/physical-engine.js')), false);
  // Move the closure outside the writer tree so missing dependencies cannot be
  // silently satisfied by the parent's full SDK node_modules directory.
  execFileSync(executable, ['-e', `
    const path = require('node:path');
    const assert = require('node:assert/strict');
    const root = process.argv[1];
    const load = require('node:module').createRequire(path.join(root, 'dist/physical-discovery.js'));
    load('./physical-discovery.js');
    for (const name of ['drivelist', '@balena/apple-plist', 'sax']) {
      assert.ok(load.resolve(name).startsWith(root + path.sep));
      load(name);
    }
  `, isolated], { stdio: 'inherit', env: { ...process.env, NODE_PATH: '' } });
  console.log('Staged writer and isolated read-only inspector dependencies loaded; no disks accessed.');
} finally {
  const resolved = fs.realpathSync(directory);
  if (path.dirname(resolved) !== temporaryRoot || !path.basename(resolved).startsWith('omarchy-media-staging-')) throw new Error('Unexpected staging cleanup path.');
  fs.rmSync(resolved, { recursive: true, force: true });
}
