// Read-only native integration check. No USB is required; no device is opened,
// unmounted, ejected or written. Report counts without hardware serials or paths.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
if (process.platform === 'linux' || process.platform === 'darwin') {
  const { inventory, probePhysical } = require('../dist/physical-discovery.js');
  (async () => {
    const probe = await probePhysical();
    assert.equal(probe.available, true, probe.reason);
    const source = fs.realpathSync(path.join(__dirname, '..', 'package.json'));
    const drives = await inventory(source);
    const sourceDisks = drives.filter(drive => drive.public.reasons.includes('SOURCE_DEVICE'));
    const systemDisks = drives.filter(drive => drive.public.reasons.includes('SYSTEM_DEVICE'));
    assert.ok(sourceDisks.length > 0, 'The real source filesystem must map to a protected disk.');
    assert.ok(systemDisks.length > 0, 'The real running system must map to a protected disk.');
    assert.ok(sourceDisks.every(drive => !drive.public.eligible));
    console.log(JSON.stringify({ platform: process.platform, discovered: drives.length,
      protectedSourceDisks: sourceDisks.length, protectedSystemDisks: systemDisks.length, readOnly: true }));
  })().catch(error => { console.error(error.message); process.exitCode = 1; });
} else {
  console.log('POSIX discovery check skipped on this host.');
}
