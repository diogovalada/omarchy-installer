const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');

test('read-only inspector rejects writes and malformed commands before discovery', () => {
  for (const command of [
    { protocol: 1, action: 'write', sourcePath: 'invalid', target: {} },
    { protocol: 1, action: 'probe' },
    { protocol: 1, action: 'list', sourcePath: 'invalid', executable: 'untrusted' },
    { protocol: 1, action: 'list' },
  ]) {
    const result = spawnSync(process.execPath, [path.resolve(__dirname, '../dist/inspection-cli.js')], {
      input: JSON.stringify(command) + '\n', encoding: 'utf8', timeout: 10000, windowsHide: true,
    });
    assert.equal(result.status, 1);
    const error = JSON.parse(result.stdout);
    assert.equal(error.type, 'error');
    assert.equal(error.possiblyModified, false);
  }
});
